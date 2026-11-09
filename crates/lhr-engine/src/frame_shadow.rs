#![forbid(unsafe_code)]
//! Shadow frame carrier (PC LS3DF.dll e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6,
//! RVAs 0x50950-0x51a20: lifecycle + open/setters).
//!
//! Differential tiers rest on Ghidra bodies (ctor 129/~9, dtor 66/~13, SetTo
//! 339/~42, FindParts 109/~22, LinkTo 15/~4, UpdateV 20/~7, DeleteFromAnyShadow
//! 35/~8, Close 74/13, Open 188/32); Duplicate/SetShadowLock/SetRange/
//! SetMaxTextureDistance/SetColor/SetReverseFactor/SetOrthogonal/SetTextureSize
//! are contract tier (entry/mid/RET + linear-decode probes, same standard as
//! committed 0x2b830). Names `I3D_shadow`/members are mafia-re
//! (I3d_shadow.cpp) / PS2 working names, never original symbols: every `FUN_`
//! below is a Ghidra default. The native `m_Type == FRAME_SHADOW (0xf)` word
//! (CMP probes at 0x10050c87 and 0x1005144c) has no host kind home: no
//! `FrameKind` variant is added and shadow-ness is `LoadedFrame.shadow.is_some()`,
//! following the mirror carrier precedent (`create_mirror_frame` keeps
//! `FrameKind::Model`).
//! Precision policy: lifecycle performs no float arithmetic — only f32 bitwise
//! copies (`FLD`/`FSTP`) and 32-bit literal stores. Setter clamps use ordered
//! f32 comparisons (never `clamp`, NaN bits preserved); bit-preserving raw
//! stores where the original stores before comparing.

use std::sync::Arc;

use crate::frame_transform::{FrameKey, I3dFrameState};
use crate::model_loading::{ModelArena, ModelError};
use crate::scene_ownership::{SceneKey, SCENE_OK};

/// Host analog of `FRMFLAGS_SHADOW` (`0x8000`) in the *graph* `m_FrmFlags`
/// domain — distinct from `shadow_pass::SHDF_NONE` (`0x8000`) in the `shd_flags`
/// domain. Same value, different words (both immediates-evidenced).
pub const SHADOW_MEMBER_FLAG: u32 = 0x8000;
/// Miss word of the Duplicate guard path (`MOV EAX,0x80000007` at 0x10050d23).
pub const SHADOW_INVALID_PARAMS: u32 = 0x8000_0007;
/// Alloc-null word of the Open path (`MOV EAX,0x80000002` at 0x1005179f).
pub const SHADOW_OUT_OF_MEM: u32 = 0x8000_0002;
/// Host analog of `I3DLOCK_NONE` (PS2/mafia-re name; ctor stores lock 0).
pub const SHADOW_LOCK_NONE: u32 = 0;

/// Device/visual lifetime of one shadow stays a backend contract: the
/// `0x26c`-byte visual allocation/ctor (0x50950), `Release` (0x50b90) and the
/// `LinkTo(NULL,0)` unlink (0x50d80) never touch host state here.
pub trait ShadowVisualBackend {
    fn create_shadow_visual(&mut self, shadow: FrameKey) -> Result<(), ModelError>;
    fn release_shadow_visual(&mut self, shadow: FrameKey) -> Result<(), ModelError>;
    fn unlink_shadow_visual(&mut self, shadow: FrameKey) -> Result<(), ModelError>;
}

/// Device texture effects of Close/Open/SetTextureSize (Release/retain and the
/// `CreateITexture`/`Create` pair). No direct GPU calls here; failures keep
/// the backend `ModelError` carrier (no erasure into state words).
pub trait ShadowTextureBackend {
    fn release_shadow_texture(
        &mut self,
        texture: crate::visual_resources::TextureKey,
    ) -> Result<(), ModelError>;
}

/// File-owned ShadowDef load (LoadShadow 0x60a50). No direct file calls here.
pub enum ShadowLoadError {
    OutOfMemory,
    Failed(u32),
}
pub trait ShadowDefBackend {
    fn load_shadow_def(
        &mut self,
        name: &[u8],
        flags: u32,
    ) -> Result<ShadowDefData, ShadowLoadError>;
}

/// One file-loaded shadow geometry part (native `S_shadow_part`, stride 12 B:
/// name pointer, vert count, face count — 32-bit words).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShadowPart {
    pub name: Vec<u8>,
    pub num_verts: u32,
    pub num_faces: u32,
}

/// File-loaded shadow definition (native `ShadowDef`). Plain file data;
/// the per-part name deletes + three vector clears run in `Close` order
/// (shadow_pass drop plan composes them; host `Vec` drop stays host).
#[derive(Clone, Debug, Default)]
pub struct ShadowDefData {
    pub parts: Vec<ShadowPart>,
    pub verts: Vec<[f32; 3]>,
    pub faces: Vec<u16>,
}

/// One geometry member (native `S_shadow_frame`, stride 8 B: frame pointer,
/// vert count).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShadowMemberFrame {
    pub frame: Option<FrameKey>,
    pub num_verts: u32,
}

/// Derived shadow state carried on a `FrameKind::Model` frame (mirror-carrier
/// precedent; counts kept as `i32` only where the `-1` sentinel is
/// stored/compared natively).
#[derive(Clone, Debug)]
pub struct I3dShadowState {
    pub(crate) def: Option<Arc<ShadowDefData>>,
    pub(crate) max_texture_distance: f32,
    pub(crate) range: f32,
    pub(crate) size_x: u32,
    pub(crate) size_y: u32,
    pub(crate) visible_range: f32,
    pub(crate) reverse_factor: f32,
    pub(crate) color: [f32; 3],
    pub(crate) lock: u32,
    pub(crate) orthogonal: bool,
    pub(crate) light_dir: [f32; 3],
    /// UpdateLight-owned (later group renders/stores it); kept here so the
    /// ctor shape stays complete.
    #[allow(dead_code)]
    pub(crate) opacity: f32,
    pub(crate) scene: Option<SceneKey>,
    pub(crate) verts: Vec<[f32; 3]>,
    pub(crate) world_verts: Vec<[f32; 3]>,
    pub(crate) points: Vec<[f32; 2]>,
    pub(crate) faces: Vec<u16>,
    pub(crate) members: Vec<ShadowMemberFrame>,
    pub(crate) texture: Option<crate::visual_resources::TextureKey>,
    pub(crate) shadow_flags: u32,
    pub(crate) verts_count: i32,
    pub(crate) lights_count: i32,
}

impl I3dShadowState {
    /// Original ctor field projection, PC RVA 0x50950 (differential: Ghidra
    /// body 542 B, 129 insn/~9 BB; mafia-re I3d_shadow.cpp:36-76). Every
    /// literal below is a decoded store from that body. Unmodeled (no host
    /// home, disclosed): the `sector` cache word (NULL at ctor; UpdateLight
    /// compares/refreshes it — the pass takes sector readiness as a caller
    /// gate instead), the `light_matrix` Identity store, and the visual
    /// back-link (`visual->shadow = this`, backend-owned).
    pub(crate) fn constructor() -> Self {
        Self {
            def: None,
            max_texture_distance: 1.0,
            range: 8.0,
            size_x: 64,
            size_y: 64,
            visible_range: 100.0,
            reverse_factor: 0.5,
            color: [0.0; 3],
            lock: SHADOW_LOCK_NONE,
            orthogonal: true,
            light_dir: [0.0, 0.0, 1.0],
            opacity: 1.0,
            scene: None,
            verts: Vec::new(),
            world_verts: Vec::new(),
            points: Vec::new(),
            faces: Vec::new(),
            members: Vec::new(),
            texture: None,
            shadow_flags: 0,
            verts_count: -1,
            lights_count: -1,
        }
    }
    /// Native `SetReverseFactor` clamp (`[0,1]`, PC RVA 0x51980): raw store
    /// first, then ordered limbs; NaN keeps raw bits via the comparison shape.
    #[allow(clippy::manual_clamp)]
    fn set_reverse(&mut self, f: f32) {
        self.reverse_factor = f;
        if self.reverse_factor < 0.0 {
            self.reverse_factor = 0.0;
        } else if self.reverse_factor > 1.0 {
            self.reverse_factor = 1.0;
        }
    }
    /// Native `SetColor` clamp (PC RVA 0x518b0), same limb shape.
    #[allow(clippy::manual_clamp)]
    fn set_color(&mut self, c: [f32; 3]) {
        self.color = c;
        for lane in &mut self.color {
            if *lane < 0.0 {
                *lane = 0.0;
            } else if *lane > 1.0 {
                *lane = 1.0;
            }
        }
    }
    /// Native `SetShadowLock` stores (PC RVA 0x51400, contract: exact 6-insn
    /// walk): lock, flags 0, lights −1, in order.
    fn apply_lock(&mut self, lock: u32) {
        self.lock = lock;
        self.shadow_flags = 0;
        self.lights_count = -1;
    }
    /// Native `SetOrthogonal` stores (PC RVA 0x519e0): flag, lights −1,
    /// light-dir zero, flags 0. The native byte stores raw (any nonzero
    /// truthy); the host `bool` normalizes (disclosed deviation).
    fn apply_orthogonal(&mut self, orthogonal: bool) {
        self.orthogonal = orthogonal;
        self.lights_count = -1;
        self.light_dir = [0.0; 3];
        self.shadow_flags = 0;
    }
}

/// Helper-phase accumulator for the FindParts walk (PC RVA 0x51260). Host
/// phase only — receives no function credit. Replaces the file-static
/// `FoundFrames`/`SetToNumFaces`/`SetToNumVerts` words (documented deviation:
/// reentrancy-safe, no process-shared state).
#[derive(Debug, Default)]
struct ShadowFindAccumulator {
    matches: Vec<(FrameKey, usize)>,
    total_faces: u32,
    total_verts: u32,
}

impl ModelArena {
    fn shadow_state(&self, key: FrameKey) -> Result<&I3dShadowState, ModelError> {
        self.frame(key)?
            .shadow
            .as_ref()
            .ok_or(ModelError::ShadowFrameRequired(key))
    }
    fn shadow_state_mut(&mut self, key: FrameKey) -> Result<&mut I3dShadowState, ModelError> {
        let index = self.frame_index(key)?;
        self.frames[index]
            .shadow
            .as_mut()
            .ok_or(ModelError::ShadowFrameRequired(key))
    }
    pub fn shadow(&self, key: FrameKey) -> Result<&I3dShadowState, ModelError> {
        self.shadow_state(key)
    }

    /// Original ctor, PC RVA 0x50950 (differential). Order: insert carrier →
    /// registry reserve → backend visual create → store derived state → push
    /// registry (native `push_back` order). A backend/visual OOM leaves the
    /// inserted frame retained and reports `AllocationOverflow` (native calls
    /// the fatal `ErrorHandler`; the host has no fatal path — disclosed).
    pub fn create_shadow_frame(
        &mut self,
        visual: &mut impl ShadowVisualBackend,
    ) -> Result<FrameKey, ModelError> {
        let state = I3dFrameState::model();
        debug_assert!(state.kind == crate::frame_transform::FrameKind::Model);
        let key = self.insert(state, None, Vec::new(), Vec::new(), None)?;
        self.shadow_registry
            .try_reserve(1)
            .map_err(|_| ModelError::AllocationOverflow)?;
        visual.create_shadow_visual(key)?;
        let index = self.frame_index(key)?;
        self.frames[index].shadow = Some(I3dShadowState::constructor());
        self.shadow_registry.push(key);
        Ok(key)
    }

    /// Original dtor, PC RVA 0x50b90 (differential: 206 B, 66 insn/~13 BB).
    /// Order-exact: `Close` (member/geometry frees, def + texture release,
    /// 0/-1/-1 tail, through the same device backend as `Close`) → scene
    /// detach → visual release → registry swap-remove → drop derived state.
    /// The base-dtor tail stays with the base producer (disclosed). A failing
    /// backend call propagates with the reached prefix retained (native
    /// `Release` cannot fail — disclosed divergence, no invented rollback).
    pub fn destroy_shadow_derived(
        &mut self,
        key: FrameKey,
        visual: &mut impl ShadowVisualBackend,
        tex: &mut impl ShadowTextureBackend,
    ) -> Result<(), ModelError> {
        self.shadow_state(key)?;
        self.shadow_close(key, tex)?;
        let scene = self.shadow_state(key)?.scene;
        if let Some(scene) = scene {
            self.scene_delete_shadow(scene, key)?;
        }
        visual.release_shadow_visual(key)?;
        if let Some(pos) = self.shadow_registry.iter().position(|&f| f == key) {
            self.shadow_registry.swap_remove(pos);
        }
        let index = self.frame_index(key)?;
        self.frames[index].shadow = None;
        Ok(())
    }

    /// Member/geometry teardown shared by the dtor, `Close`, and the `SetTo`
    /// teardown: member-flag clears, frees and cache resets, in native order.
    /// The file `def` is NOT dropped here: `SetTo` reads it right after the
    /// teardown (only `Close`/the dtor release it — separate step below).
    /// `points` is NOT freed here either: native `SetTo` never frees points
    /// (it keeps/leaks the old buffer and allocates new storage); only
    /// `Close`/the dtor free them (separate step below).
    fn clear_shadow_members(&mut self, key: FrameKey) -> Result<(), ModelError> {
        let members = self.shadow_state(key)?.members.clone();
        for member in &members {
            if let Some(frame) = member.frame {
                self.graph.change_flags(frame, SHADOW_MEMBER_FLAG, 0)?;
            }
        }
        let state = self.shadow_state_mut(key)?;
        state.members.clear();
        state.verts.clear();
        state.world_verts.clear();
        state.faces.clear();
        Ok(())
    }

    /// Original Close, PC RVA 0x514e0 (differential: 74 insn/13 BB). Ordered:
    /// member-flag loop over frames (stride-8 records) → free frames/verts/
    /// faces → def inline release (Arc drop = host analog of the refcount
    /// path) → free points → texture Release via the device backend →
    /// flags/counts tail (0/−1/−1). Nulls are silent skips, exactly as
    /// natively. World-verts clear rides with the verts free; native `Close`
    /// keeps the stale world-verts pointer ("the original's too") — the host
    /// clears instead of dangling (disclosed hardening).
    pub fn shadow_close(
        &mut self,
        key: FrameKey,
        backend: &mut impl ShadowTextureBackend,
    ) -> Result<(), ModelError> {
        self.shadow_state(key)?;
        self.clear_shadow_members(key)?;
        // Def inline release (Arc drop = host analog of the refcount path).
        self.shadow_state_mut(key)?.def = None;
        self.shadow_state_mut(key)?.points.clear();
        if let Some(texture) = self.shadow_state(key)?.texture {
            backend.release_shadow_texture(texture)?;
            self.shadow_state_mut(key)?.texture = None;
        }
        let state = self.shadow_state_mut(key)?;
        state.shadow_flags = 0;
        state.verts_count = -1;
        state.lights_count = -1;
        Ok(())
    }

    /// Original Open, PC RVA 0x515e0 (differential: 188 insn/32 BB, SEH).
    /// Order: Close first → alloc def (None = OUTOFMEM, store nothing) →
    /// LoadShadow passthrough (failed code returns with the empty def kept,
    /// "no cleanup", exactly as natively) → parent SetTo → OK.
    pub fn shadow_open(
        &mut self,
        key: FrameKey,
        name: &[u8],
        flags: u32,
        tex: &mut impl ShadowTextureBackend,
        defb: &mut impl ShadowDefBackend,
        visual: &mut impl ShadowVisualBackend,
    ) -> Result<u32, ModelError> {
        self.shadow_state(key)?;
        self.shadow_close(key, tex)?;
        let data = match defb.load_shadow_def(name, flags) {
            Ok(data) => data,
            Err(ShadowLoadError::OutOfMemory) => return Ok(SHADOW_OUT_OF_MEM),
            Err(ShadowLoadError::Failed(code)) => {
                self.shadow_state_mut(key)?.def = Some(Arc::new(ShadowDefData::default()));
                return Ok(code);
            }
        };
        self.shadow_state_mut(key)?.def = Some(Arc::new(data));
        if self.links(key)?.parent.is_some() {
            let parent = self.links(key)?.parent;
            self.set_shadow_to(key, parent, visual)?;
        }
        Ok(SCENE_OK)
    }

    /// Loader hook for file-data publication. `Arc` clone is the host analog
    /// of the native `def->ref++` share.
    pub fn set_shadow_def(
        &mut self,
        key: FrameKey,
        def: Option<Arc<ShadowDefData>>,
    ) -> Result<(), ModelError> {
        self.shadow_state(key)?;
        self.shadow_state_mut(key)?.def = def;
        Ok(())
    }

    /// Original Duplicate, PC RVA 0x50c60 (contract: no Ghidra body,
    /// `LAB_10050c60`/`not_in_procedure`; order from the entry/mid/RET probes
    /// and linear decode 0x10050c60–0x10050d29). Null/self → `0x80000007` with no
    /// effects (unknown-key admission stands in for the null-pointer guard);
    /// base `duplicate_frame` first; non-shadow source → base-only `Ok`.
    /// NOTE vs mafia-re: the binary routes four derived virtuals
    /// (reverse-factor, color, lock, orthogonal — slots `+0x68/+0x6c/+0x64/+0x70`)
    /// and copies `visible_range` directly (`FLD`/`FSTP`); no `SetVisibleRange`
    /// call is in the byte walk (disclosed divergence). The native `Close()`
    /// prefix (member/geometry frees, def + texture release) runs after the
    /// base copy and before the type check, exactly once — the destination
    /// residue from a previous life is gone on every non-guard path.
    pub fn duplicate_shadow(
        &mut self,
        dst: FrameKey,
        src: FrameKey,
        backend: &mut impl ShadowTextureBackend,
    ) -> Result<u32, ModelError> {
        if self.frame(dst).is_err() || self.frame(src).is_err() || dst == src {
            return Ok(SHADOW_INVALID_PARAMS);
        }
        self.duplicate_frame(dst, src)?;
        self.shadow_close(dst, backend)?;
        if self.frame(src)?.shadow.is_none() {
            return Ok(SCENE_OK);
        }
        let (def, max_tex, range, sx, sy, visible) = {
            let s = self.shadow_state(src)?;
            (
                s.def.clone(),
                s.max_texture_distance,
                s.range,
                s.size_x,
                s.size_y,
                s.visible_range,
            )
        };
        let (reverse, color, lock, orthogonal) = {
            let s = self.shadow_state(src)?;
            (s.reverse_factor, s.color, s.lock, s.orthogonal)
        };
        {
            let d = self.shadow_state_mut(dst)?;
            // A shadow destination keeps its own copies list/geometry; only the
            // file-data words travel (native frees dst arrays via Close first —
            // the host Close-prefix runs here, exactly once).
            d.def = def;
            d.max_texture_distance = max_tex;
            d.range = range;
            d.size_x = sx;
            d.size_y = sy;
            d.visible_range = visible;
        }
        // Virtual-call order from the byte walk: reverse, color, lock, orthogonal.
        self.shadow_state_mut(dst)?.set_reverse(reverse);
        self.shadow_state_mut(dst)?.set_color(color);
        self.shadow_state_mut(dst)?.apply_lock(lock);
        self.shadow_state_mut(dst)?.apply_orthogonal(orthogonal);
        Ok(SCENE_OK)
    }

    /// Original SetTo, PC RVA 0x50d80 (differential: 1237 B, 339 insn/~42 BB).
    /// Order: null-target scene detach → visual unlink → member-flag teardown
    /// → free-all → `def`/target gate → find (level-1 inline merged exactly
    /// into the recursion: union equals `FindParts(target)`) → zero-faces
    /// return → five reserves in native order → base-biased copy.
    /// Native `verts`/`points` are uninitialized; the host zero-fills
    /// (disclosed). File face indices read as per-part LOCAL (0-based): the
    /// single bias ADD rebases them to the copied prefix (wrapping u16);
    /// file-global indices would need a rebase the binary does not perform
    /// (disclosed layout reading). Reserves use `try_reserve` in native alloc
    /// order (plus `world_verts`, which shares the vert quota); on overflow
    /// all five are cleared (host analog of the delete-quadruple, extended to
    /// the split world buffer) before `AllocationOverflow` (native fatals —
    /// disclosed; native additionally nulls `verts` twice and never nulls
    /// `points` — the host clears all five uniformly, disclosed deviation).
    /// No rollback after the reserves: the copy loop's member-flag
    /// effects stay on graph-error paths, exactly as natively (disclosed).
    pub fn set_shadow_to(
        &mut self,
        shadow: FrameKey,
        target: Option<FrameKey>,
        visual: &mut impl ShadowVisualBackend,
    ) -> Result<(), ModelError> {
        self.shadow_state(shadow)?;
        if target.is_none() {
            if let Some(scene) = self.shadow_state(shadow)?.scene {
                self.scene_delete_shadow(scene, shadow)?;
                // Native DeleteShadow clears the back-pointer.
                self.shadow_state_mut(shadow)?.scene = None;
            }
        }
        visual.unlink_shadow_visual(shadow)?;
        self.clear_shadow_members(shadow)?;
        {
            let state = self.shadow_state_mut(shadow)?;
            state.shadow_flags = 0;
            state.verts_count = -1;
            state.lights_count = -1;
        }
        let Some(target) = target else { return Ok(()) };
        self.frame(target)?;
        let def = self.shadow_state(shadow)?.def.clone();
        let Some(def) = def else { return Ok(()) };
        let mut acc = ShadowFindAccumulator::default();
        self.find_shadow_parts_recursive(target, &def, &mut acc)?;
        if acc.total_faces == 0 {
            return Ok(());
        }
        // File-def consistency gate (library hardening, disclosed): a corrupt
        // backend must report, never panic the host on OOB indexing below.
        // Native would fault on the overread — fail-stop either way.
        let (need_faces, need_verts) = {
            let mut ff = 0usize;
            let mut fv = 0usize;
            for p in &def.parts {
                ff += p.num_faces as usize * 3;
                fv += p.num_verts as usize;
            }
            (ff, fv)
        };
        if def.faces.len() < need_faces || def.verts.len() < need_verts {
            return Err(ModelError::AllocationOverflow);
        }
        let n_verts = acc.total_verts as usize;
        if self
            .shadow_state_mut(shadow)?
            .faces
            .try_reserve(acc.total_faces as usize * 3)
            .is_err()
            || self
                .shadow_state_mut(shadow)?
                .verts
                .try_reserve(n_verts)
                .is_err()
            || self
                .shadow_state_mut(shadow)?
                .world_verts
                .try_reserve(n_verts)
                .is_err()
            || self
                .shadow_state_mut(shadow)?
                .points
                .try_reserve(n_verts)
                .is_err()
            || self
                .shadow_state_mut(shadow)?
                .members
                .try_reserve(acc.matches.len())
                .is_err()
        {
            // Host analog of the native delete-quadruple: clear all five.
            let state = self.shadow_state_mut(shadow)?;
            state.faces.clear();
            state.verts.clear();
            state.world_verts.clear();
            state.points.clear();
            state.members.clear();
            return Err(ModelError::AllocationOverflow);
        }
        // Fallible phase done; from here only infallible stores run, then the
        // member-flag effects in match order (native copy-loop order). Points
        // are cleared + resized here (a fresh buffer, never the torn-down
        // prefix — native allocates new storage rather than reusing).
        {
            let state = self.shadow_state_mut(shadow)?;
            state.faces.clear();
            state.faces.resize(acc.total_faces as usize * 3, 0);
            state.verts.clear();
            state.verts.resize(n_verts, [0.0; 3]);
            state.world_verts.clear();
            state.world_verts.resize(n_verts, [0.0; 3]);
            state.points.clear();
            state.points.resize(n_verts, [0.0; 2]);
            state.members.clear();
        }
        let mut face_cursor = 0usize;
        let mut vert_cursor = 0usize;
        let mut vert_base: u16 = 0;
        for (frame, part) in acc.matches {
            let (first_vert, first_face, part_verts, part_faces) = {
                let mut fv = 0u32;
                let mut ff = 0u32;
                for p in &def.parts[..part] {
                    fv += p.num_verts;
                    ff += p.num_faces;
                }
                let p = &def.parts[part];
                (
                    fv as usize,
                    ff as usize,
                    p.num_verts as usize,
                    p.num_faces as usize,
                )
            };
            for i in 0..part_faces * 3 {
                let biased = def.faces[first_face * 3 + i].wrapping_add(vert_base);
                let state = self.shadow_state_mut(shadow)?;
                state.faces[face_cursor + i] = biased;
            }
            face_cursor += part_faces * 3;
            {
                let state = self.shadow_state_mut(shadow)?;
                state.verts[vert_cursor..vert_cursor + part_verts]
                    .copy_from_slice(&def.verts[first_vert..first_vert + part_verts]);
            }
            vert_cursor += part_verts;
            vert_base = vert_base.wrapping_add(part_verts as u16);
            self.shadow_state_mut(shadow)?
                .members
                .push(ShadowMemberFrame {
                    frame: Some(frame),
                    num_verts: part_verts as u32,
                });
            self.graph.change_flags(frame, 0, SHADOW_MEMBER_FLAG)?;
        }
        Ok(())
    }

    /// Original FindParts search, PC RVA 0x51260 (differential: 361 B,
    /// 109 insn/~22 BB; self-recursion evidenced in callees). Recursive to
    /// mirror the native walk. Null original names compare as empty (host
    /// admission for the `0x100af7fc` default substitution — disclosed).
    /// First name hit per frame wins; totals accumulate per hit (wrapping,
    /// like the native `ADD`). Unbounded recursion mirrors native `FindParts`
    /// exactly (same order, same depth behavior — disclosed, no worklist).
    /// Model-prefix stripping is unmodeled: names compare whole
    /// `original_name_bytes` (native matches without the model's prefix —
    /// disclosed gap for prefixed hierarchies).
    fn find_shadow_parts_recursive(
        &mut self,
        frame: FrameKey,
        def: &ShadowDefData,
        acc: &mut ShadowFindAccumulator,
    ) -> Result<(), ModelError> {
        let mut child = self.links(frame)?.child;
        while let Some(current) = child {
            child = self.links(current)?.next;
            if self.state(current)?.flags & 1 == 0 {
                continue;
            }
            let name = self
                .frame(current)?
                .original_name_bytes
                .clone()
                .unwrap_or_default();
            let name = name.as_slice();
            for (index, part) in def.parts.iter().enumerate() {
                if name == part.name.as_slice() {
                    acc.matches
                        .try_reserve(1)
                        .map_err(|_| ModelError::AllocationOverflow)?;
                    acc.matches.push((current, index));
                    acc.total_faces = acc.total_faces.wrapping_add(part.num_faces);
                    acc.total_verts = acc.total_verts.wrapping_add(part.num_verts);
                    break;
                }
            }
            self.find_shadow_parts_recursive(current, def, acc)?;
        }
        Ok(())
    }

    /// Original LinkTo, PC RVA 0x513d0 (differential: 35 B, 15 insn/~4 BB —
    /// `SetTo` then base `LinkTo`). Mirrors `link_occluder_frame_derived`.
    pub fn link_shadow_frame(
        &mut self,
        child: FrameKey,
        parent: Option<FrameKey>,
        flags: u32,
        visual: &mut impl ShadowVisualBackend,
    ) -> Result<u32, ModelError> {
        self.shadow_state(child)?;
        if let Some(parent) = parent {
            self.frame(parent)?;
        }
        self.set_shadow_to(child, parent, visual)?;
        self.link_frame_base(child, parent, flags)
    }

    /// Original SetShadowLock, PC RVA 0x51400 (contract: exact 6-insn walk,
    /// `LAB_10051400`/`not_in_procedure` + vtable DATA xref `0x1009cba4`).
    pub fn set_shadow_lock(&mut self, key: FrameKey, lock: u32) -> Result<(), ModelError> {
        self.shadow_state(key)?;
        self.shadow_state_mut(key)?.apply_lock(lock);
        Ok(())
    }

    /// Original UpdateV, PC RVA 0x51430 (differential: 78 B, 20 insn/~7 BB;
    /// callee `AddShadow` 0x1004a5a0). Scene-present early-out, then the
    /// parent walk for the first sector ancestor. A sector without a scene
    /// returns `Ok(())` (native would fault — disclosed, no invented crash).
    pub fn update_shadow_v(&mut self, shadow: FrameKey) -> Result<(), ModelError> {
        if self.shadow_state(shadow)?.scene.is_some() {
            return Ok(());
        }
        let mut at = self.links(shadow)?.parent;
        while let Some(frame) = at {
            at = self.links(frame)?.parent;
            if self.state(frame)?.kind == crate::frame_transform::FrameKind::Sector {
                let Some(scene) = self.sector(crate::scene_ownership::SectorKey(frame))?.scene
                else {
                    return Ok(());
                };
                self.scene_add_shadow(scene, shadow)?;
                let state = self.shadow_state_mut(shadow)?;
                // Native AddShadow stores shadow->scene (I3d_scene.cpp:596-602);
                // the early-out, SetTo-None and dtor detach arms read it.
                state.scene = Some(scene);
                state.shadow_flags = 0;
                state.lights_count = -1;
                return Ok(());
            }
        }
        Ok(())
    }

    /// Original DeleteFromAnyShadow, PC RVA 0x51480 (differential: 91 B,
    /// 35 insn/~8 BB). Registry (insertion) order walk; per member: flag
    /// clear + null slot. Live re-reads each step (no callbacks run here).
    /// Driver 0x1a0e0 `I3D_driver::DeleteFromAllShadows` (mafia-re
    /// `ls3df/I3D/I3d_driver.cpp:1754`, evidenced PS2 original name) is a
    /// tail call to this routine. Wiring into the base-destruction path
    /// (next to `remove_from_any_target`) is a later step (OPEN).
    /// Driver 0x1a0e0 DeleteFromAllShadows (mafia-re
    /// `ls3df/I3D/I3d_driver.cpp:1754`, evidenced PS2 original name): pure
    /// tail call to DeleteFromAnyShadow (0x51480) above; no independent
    /// state, so the driver thunk owns this entry (not the shared callee).
    pub fn driver_delete_from_all_shadows(&mut self, frame: FrameKey) -> Result<(), ModelError> {
        self.delete_frame_from_any_shadow(frame)
    }
    pub fn delete_frame_from_any_shadow(&mut self, frame: FrameKey) -> Result<(), ModelError> {
        self.frame_index(frame)?;
        let registry = self.shadow_registry.clone();
        for shadow in registry {
            let members = self.shadow_state(shadow)?.members.clone();
            for (index, member) in members.iter().enumerate() {
                if member.frame == Some(frame) {
                    self.graph.change_flags(frame, SHADOW_MEMBER_FLAG, 0)?;
                    self.shadow_state_mut(shadow)?.members[index].frame = None;
                }
            }
        }
        Ok(())
    }

    /// Original SetRange, PC RVA 0x51800 (contract: no Ghidra body; sweep
    /// 0x10051800–0x10051839). Ordered `r > 0.0` store (NaN keeps the old
    /// range); light-dir zero + flags 0 run on every path.
    pub fn shadow_set_range(&mut self, key: FrameKey, r: f32) -> Result<(), ModelError> {
        let state = self.shadow_state_mut(key)?;
        if r > 0.0 {
            state.range = r;
        }
        state.light_dir = [0.0; 3];
        state.shadow_flags = 0;
        Ok(())
    }

    /// Working name for `I3D_shadow::SetVisibleRange` (mafia-re
    /// ls3df/I3D/I3D.h:2587-2591). No LS3DF FUNCTION address exists for it:
    /// it is a header inline compiled into LS3DF (`Duplicate` copies
    /// `visible_range` directly with `FLD`/`FSTP` instead of calling it, per
    /// the 0x50c60 byte walk). Unconditional store of the caller's range;
    /// no validation, no flag or light effects.
    pub fn set_shadow_visible_range(&mut self, key: FrameKey, range: f32) -> Result<(), ModelError> {
        self.shadow_state_mut(key)?.visible_range = range;
        Ok(())
    }

    /// Original SetMaxTextureDistance, PC RVA 0x51840 (contract: sweep
    /// 0x10051840–0x1005189e with the `80000007` fail block at 0x10051899).
    /// Ordered `d < 0 || d > 32` reject; NaN passes both ordered comparisons
    /// (false) and is STORED with effects, exactly per the reference
    /// (mafia-re I3d_shadow.cpp:379 — no Ghidra body, so the reference is the
    /// only source and it stores). Success stores + zero + `lights = -1` +
    /// flags 0.
    pub fn shadow_set_max_texture_distance(
        &mut self,
        key: FrameKey,
        d: f32,
    ) -> Result<u32, ModelError> {
        if d < 0.0 || d > 32.0 {
            return Ok(SHADOW_INVALID_PARAMS);
        }
        let state = self.shadow_state_mut(key)?;
        state.max_texture_distance = d;
        state.light_dir = [0.0; 3];
        state.lights_count = -1;
        state.shadow_flags = 0;
        Ok(SCENE_OK)
    }

    /// Original SetColor, PC RVA 0x518b0 (contract: sweep 0x100518b0–0x10051977,
    /// three ordered `[0,1]` limbs). Copy-then-clamp per limb in place; NaN
    /// retained per limb; tail flags 0 + `lights = -1`.
    #[allow(clippy::manual_clamp)]
    pub fn shadow_set_color(&mut self, key: FrameKey, c: [f32; 3]) -> Result<(), ModelError> {
        let state = self.shadow_state_mut(key)?;
        state.set_color(c);
        state.shadow_flags = 0;
        state.lights_count = -1;
        Ok(())
    }

    /// Original SetReverseFactor, PC RVA 0x51980 (contract: sweep with split
    /// RETs). Raw store FIRST; ordered clamps; NaN retains raw bits; flags 0
    /// on every path.
    #[allow(clippy::manual_clamp)]
    pub fn shadow_set_reverse_factor(&mut self, key: FrameKey, f: f32) -> Result<(), ModelError> {
        let state = self.shadow_state_mut(key)?;
        state.set_reverse(f);
        state.shadow_flags = 0;
        Ok(())
    }

    /// Original SetOrthogonal, PC RVA 0x519e0 (contract: 8-insn sweep).
    /// Byte store; order lights → zero → flags (mafia-re I3d_shadow.cpp:421-429).
    pub fn shadow_set_orthogonal(&mut self, key: FrameKey, b: bool) -> Result<(), ModelError> {
        let state = self.shadow_state_mut(key)?;
        state.apply_orthogonal(b);
        Ok(())
    }

    /// Original SetTextureSize, PC RVA 0x51a20 (contract: sweep 0x10051a20–
    /// 0x10051aaa with the `80000007` fail block at 0x10051aa5). Signed
    /// `> 128` reject on either axis; BOTH axes round up from `x` (native bug,
    /// mafia-re:442 — disclosed, kept); texture Release+null via the device
    /// backend; `lights = -1`, flags 0. Creation happens later in
    /// `RenderTexture` — this path never creates (disclosed).
    pub fn shadow_set_texture_size(
        &mut self,
        key: FrameKey,
        x: i32,
        y: i32,
        backend: &mut impl ShadowTextureBackend,
    ) -> Result<u32, ModelError> {
        if x > 128 || y > 128 {
            return Ok(SHADOW_INVALID_PARAMS);
        }
        let mut sx: i32 = 4;
        while sx < x {
            sx <<= 1;
        }
        let mut sy: i32 = 4;
        while sy < x {
            sy <<= 1;
        } // NOTE: compares x, not y — the original bug, kept bit-exact.
        let texture = self.shadow_state(key)?.texture;
        if let Some(t) = texture {
            backend.release_shadow_texture(t)?;
            self.shadow_state_mut(key)?.texture = None;
        }
        let state = self.shadow_state_mut(key)?;
        state.size_x = sx as u32;
        state.size_y = sy as u32;
        state.lights_count = -1;
        state.shadow_flags = 0;
        Ok(SCENE_OK)
    }

    /// Working name for the `range` word read: the original exposes no
    /// `I3D_shadow::GetRange` (mafia-re ls3df/I3D/visuals/I3d_shadow.cpp at
    /// pin 113b5b6 defines only `Set*` entry points; the `SetRange` setter
    /// RVA 0x51800 is already occupied). No new RVA is claimed: this only
    /// re-reads the setter-written word through the existing `shadow`
    /// accessor, with the same availability guard.
    pub fn shadow_range_of(&self, key: FrameKey) -> Result<f32, ModelError> {
        Ok(self.shadow(key)?.range)
    }

    /// Working name for the `max_texture_distance` word read: the original
    /// exposes no `I3D_shadow::GetMaxTextureDistance` (setter RVA 0x51840
    /// already occupied). No new RVA is claimed; same guard as `shadow`.
    pub fn shadow_max_texture_distance_of(&self, key: FrameKey) -> Result<f32, ModelError> {
        Ok(self.shadow(key)?.max_texture_distance)
    }

    /// Working name for the clamped `color` triple read: the original
    /// exposes no `I3D_shadow::GetColor` (setter RVA 0x518b0 already
    /// occupied). No new RVA is claimed; same guard as `shadow`.
    pub fn shadow_color_of(&self, key: FrameKey) -> Result<[f32; 3], ModelError> {
        Ok(self.shadow(key)?.color)
    }

    /// Working name for the clamped `reverse_factor` word read: the original
    /// exposes no `I3D_shadow::GetReverseFactor` (setter RVA 0x51980 already
    /// occupied). No new RVA is claimed; same guard as `shadow`.
    pub fn shadow_reverse_factor_of(&self, key: FrameKey) -> Result<f32, ModelError> {
        Ok(self.shadow(key)?.reverse_factor)
    }

    /// Working name for the `orthogonal` flag read: the original exposes no
    /// `I3D_shadow::GetOrthogonal` (setter RVA 0x519e0 already occupied).
    /// No new RVA is claimed; same guard as `shadow`.
    pub fn shadow_is_orthogonal_of(&self, key: FrameKey) -> Result<bool, ModelError> {
        Ok(self.shadow(key)?.orthogonal)
    }

    /// Working name for the rounded `size_x`/`size_y` pair read: the original
    /// exposes no `I3D_shadow::GetTextureSize` (setter RVA 0x51a20 already
    /// occupied). No new RVA is claimed; same guard as `shadow`.
    pub fn shadow_texture_size_of(&self, key: FrameKey) -> Result<[u32; 2], ModelError> {
        let state = self.shadow(key)?;
        Ok([state.size_x, state.size_y])
    }

    /// Working name for the `lock` word read: the original exposes no
    /// `I3D_shadow::GetShadowLock` (setter RVA 0x51400 already occupied).
    /// No new RVA is claimed; same guard as `shadow`.
    pub fn shadow_lock_of(&self, key: FrameKey) -> Result<u32, ModelError> {
        Ok(self.shadow(key)?.lock)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_transform::{FrameKind, I3dFrameState};

    struct NullVisual {
        log: Vec<&'static str>,
    }
    impl ShadowVisualBackend for NullVisual {
        fn create_shadow_visual(&mut self, _: FrameKey) -> Result<(), ModelError> {
            self.log.push("create");
            Ok(())
        }
        fn release_shadow_visual(&mut self, _: FrameKey) -> Result<(), ModelError> {
            self.log.push("release");
            Ok(())
        }
        fn unlink_shadow_visual(&mut self, _: FrameKey) -> Result<(), ModelError> {
            self.log.push("unlink");
            Ok(())
        }
    }
    struct NullTex {
        released: Vec<crate::visual_resources::TextureKey>,
    }
    impl ShadowTextureBackend for NullTex {
        fn release_shadow_texture(
            &mut self,
            texture: crate::visual_resources::TextureKey,
        ) -> Result<(), ModelError> {
            self.released.push(texture);
            Ok(())
        }
    }
    struct StubDef {
        mode: u8,
        code: u32,
    }
    impl ShadowDefBackend for StubDef {
        fn load_shadow_def(&mut self, _: &[u8], _: u32) -> Result<ShadowDefData, ShadowLoadError> {
            match self.mode {
                0 => Err(ShadowLoadError::OutOfMemory),
                1 => Err(ShadowLoadError::Failed(self.code)),
                _ => Ok(ShadowDefData {
                    parts: vec![ShadowPart {
                        name: b"door".to_vec(),
                        num_verts: 2,
                        num_faces: 1,
                    }],
                    verts: vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                    faces: vec![0, 1, 0],
                }),
            }
        }
    }

    fn def_two_parts() -> Arc<ShadowDefData> {
        Arc::new(ShadowDefData {
            parts: vec![
                ShadowPart {
                    name: b"door".to_vec(),
                    num_verts: 2,
                    num_faces: 1,
                },
                ShadowPart {
                    name: b"wheel".to_vec(),
                    num_verts: 3,
                    num_faces: 2,
                },
            ],
            verts: vec![
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [2.0, 0.0, 0.0],
                [0.0, 2.0, 0.0],
            ],
            // Per-part LOCAL indices (the single bias ADD rebases them to the
            // copied prefix; file-global indices would need a rebase the
            // binary does not perform — disclosed layout reading).
            faces: vec![0, 1, 0, 0, 1, 2, 0, 2, 1],
        })
    }
    fn named_child(arena: &mut ModelArena, parent: FrameKey, name: &[u8], on: bool) -> FrameKey {
        let mut state = I3dFrameState::model();
        state.kind = FrameKind::Dummy;
        let key = arena
            .insert(state, None, name.to_vec(), Vec::new(), None)
            .unwrap();
        // Test-only admission: original-name bytes equal the full name (no model prefix).
        let index = arena.frame_index(key).unwrap();
        arena.frames[index].original_name_bytes = Some(name.to_vec());
        if !on {
            arena.graph.change_flags(key, 1, 0).unwrap();
        }
        arena.link_frame_base(key, Some(parent), 0).unwrap();
        key
    }

    #[test]
    fn shadow_range_reader_reports_ctor_default_then_setter_store() {
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let key = arena.create_shadow_frame(&mut visual).unwrap();
        assert_eq!(arena.shadow_range_of(key).unwrap(), 8.0);
        arena.shadow_set_range(key, 12.0).unwrap();
        assert_eq!(arena.shadow_range_of(key).unwrap(), 12.0);
        arena.shadow_set_range(key, -1.0).unwrap();
        assert_eq!(arena.shadow_range_of(key).unwrap(), 12.0);
        assert_eq!(
            arena.shadow_range_of(FrameKey(u32::MAX)).unwrap_err(),
            ModelError::UnknownFrame(FrameKey(u32::MAX))
        );
    }

    #[test]
    fn shadow_visible_range_setter_stores_unconditionally() {
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let key = arena.create_shadow_frame(&mut visual).unwrap();
        assert_eq!(arena.shadow_state(key).unwrap().visible_range, 100.0);
        arena.set_shadow_visible_range(key, 40.0).unwrap();
        assert_eq!(arena.shadow_state(key).unwrap().visible_range, 40.0);
        // Unlike SetRange there is no positivity gate: zero, negative and
        // NaN all store.
        arena.set_shadow_visible_range(key, -2.0).unwrap();
        assert_eq!(arena.shadow_state(key).unwrap().visible_range, -2.0);
        arena.set_shadow_visible_range(key, f32::NAN).unwrap();
        assert!(arena.shadow_state(key).unwrap().visible_range.is_nan());
        assert_eq!(
            arena.set_shadow_visible_range(FrameKey(u32::MAX), 1.0).unwrap_err(),
            ModelError::UnknownFrame(FrameKey(u32::MAX))
        );
    }

    #[test]
    fn shadow_max_texture_distance_reader_reports_ctor_default_then_setter_store() {
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let key = arena.create_shadow_frame(&mut visual).unwrap();
        assert_eq!(arena.shadow_max_texture_distance_of(key).unwrap(), 1.0);
        arena.shadow_set_max_texture_distance(key, 2.5).unwrap();
        assert_eq!(arena.shadow_max_texture_distance_of(key).unwrap(), 2.5);
        arena.shadow_set_max_texture_distance(key, 33.0).unwrap();
        assert_eq!(arena.shadow_max_texture_distance_of(key).unwrap(), 2.5);
        assert_eq!(
            arena
                .shadow_max_texture_distance_of(FrameKey(u32::MAX))
                .unwrap_err(),
            ModelError::UnknownFrame(FrameKey(u32::MAX))
        );
    }

    #[test]
    fn shadow_color_reader_reports_ctor_default_then_setter_store() {
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let key = arena.create_shadow_frame(&mut visual).unwrap();
        assert_eq!(arena.shadow_color_of(key).unwrap(), [0.0; 3]);
        arena.shadow_set_color(key, [0.25, 0.5, 2.0]).unwrap();
        assert_eq!(arena.shadow_color_of(key).unwrap(), [0.25, 0.5, 1.0]);
        assert_eq!(
            arena.shadow_color_of(FrameKey(u32::MAX)).unwrap_err(),
            ModelError::UnknownFrame(FrameKey(u32::MAX))
        );
    }

    #[test]
    fn shadow_reverse_factor_reader_reports_ctor_default_then_setter_store() {
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let key = arena.create_shadow_frame(&mut visual).unwrap();
        assert_eq!(arena.shadow_reverse_factor_of(key).unwrap(), 0.5);
        arena.shadow_set_reverse_factor(key, 0.75).unwrap();
        assert_eq!(arena.shadow_reverse_factor_of(key).unwrap(), 0.75);
        arena.shadow_set_reverse_factor(key, -1.0).unwrap();
        assert_eq!(arena.shadow_reverse_factor_of(key).unwrap(), 0.0);
        assert_eq!(
            arena
                .shadow_reverse_factor_of(FrameKey(u32::MAX))
                .unwrap_err(),
            ModelError::UnknownFrame(FrameKey(u32::MAX))
        );
    }

    #[test]
    fn shadow_orthogonal_reader_reports_ctor_default_then_setter_store() {
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let key = arena.create_shadow_frame(&mut visual).unwrap();
        assert!(arena.shadow_is_orthogonal_of(key).unwrap());
        arena.shadow_set_orthogonal(key, false).unwrap();
        assert!(!arena.shadow_is_orthogonal_of(key).unwrap());
        assert_eq!(
            arena
                .shadow_is_orthogonal_of(FrameKey(u32::MAX))
                .unwrap_err(),
            ModelError::UnknownFrame(FrameKey(u32::MAX))
        );
    }

    #[test]
    fn shadow_texture_size_reader_reports_ctor_default_then_setter_store() {
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let key = arena.create_shadow_frame(&mut visual).unwrap();
        let mut tex = NullTex {
            released: Vec::new(),
        };
        assert_eq!(arena.shadow_texture_size_of(key).unwrap(), [64, 64]);
        arena.shadow_set_texture_size(key, 100, 5, &mut tex).unwrap();
        assert_eq!(arena.shadow_texture_size_of(key).unwrap(), [128, 128]);
        assert_eq!(
            arena
                .shadow_texture_size_of(FrameKey(u32::MAX))
                .unwrap_err(),
            ModelError::UnknownFrame(FrameKey(u32::MAX))
        );
    }

    #[test]
    fn shadow_lock_reader_reports_ctor_default_then_setter_store() {
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let key = arena.create_shadow_frame(&mut visual).unwrap();
        assert_eq!(arena.shadow_lock_of(key).unwrap(), SHADOW_LOCK_NONE);
        arena.set_shadow_lock(key, 7).unwrap();
        assert_eq!(arena.shadow_lock_of(key).unwrap(), 7);
        assert_eq!(
            arena.shadow_lock_of(FrameKey(u32::MAX)).unwrap_err(),
            ModelError::UnknownFrame(FrameKey(u32::MAX))
        );
    }

    #[test]
    fn ctor_defaults_match_binary_stores() {
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let key = arena.create_shadow_frame(&mut visual).unwrap();
        let s = arena.shadow_state(key).unwrap();
        assert_eq!((s.size_x, s.size_y), (64, 64));
        assert_eq!(s.range, 8.0);
        assert_eq!(s.reverse_factor, 0.5);
        assert_eq!(s.opacity, 1.0);
        assert_eq!(s.max_texture_distance, 1.0);
        assert_eq!(s.visible_range, 100.0);
        assert_eq!(s.color, [0.0; 3]);
        assert!(s.orthogonal);
        assert_eq!(s.lock, SHADOW_LOCK_NONE);
        assert_eq!(s.light_dir, [0.0, 0.0, 1.0]);
        assert_eq!((s.verts_count, s.lights_count), (-1, -1));
        assert_eq!(s.shadow_flags, 0);
        assert!(s.def.is_none() && s.scene.is_none() && s.texture.is_none());
        assert_eq!(visual.log, ["create"]);
        assert_eq!(arena.shadow_registry, [key]);
    }

    #[test]
    fn dtor_order_releases_then_detaches_then_unlists() {
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let key = arena.create_shadow_frame(&mut visual).unwrap();
        visual.log.clear();
        let mut tex = NullTex {
            released: Vec::new(),
        };
        arena
            .destroy_shadow_derived(key, &mut visual, &mut tex)
            .unwrap();
        assert_eq!(visual.log, ["release"]);
        assert!(arena.shadow_registry.is_empty());
        assert!(arena.frame(key).unwrap().shadow.is_none());
        assert_eq!(
            arena.shadow_state(key).unwrap_err(),
            ModelError::ShadowFrameRequired(key)
        );
    }

    #[test]
    fn duplicate_rejects_null_and_self_with_word_and_no_effects() {
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let key = arena.create_shadow_frame(&mut visual).unwrap();
        let before = arena.frame(key).unwrap().shadow.clone();
        let mut tex = NullTex {
            released: Vec::new(),
        };
        assert_eq!(
            arena.duplicate_shadow(key, key, &mut tex).unwrap(),
            SHADOW_INVALID_PARAMS
        );
        assert!(arena.frame(key).unwrap().shadow.is_some());
        assert_eq!(
            arena.frame(key).unwrap().shadow.as_ref().unwrap().range,
            before.as_ref().unwrap().range
        );
        let _ = &mut visual;
    }

    #[test]
    fn duplicate_shares_def_and_orders_derived_stores() {
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let src = arena.create_shadow_frame(&mut visual).unwrap();
        arena.set_shadow_def(src, Some(def_two_parts())).unwrap();
        arena.shadow_state_mut(src).unwrap().reverse_factor = 2.5;
        arena.shadow_state_mut(src).unwrap().color = [-1.0, 0.5, 2.0];
        let dst = arena.create_shadow_frame(&mut visual).unwrap();
        let mut tex = NullTex {
            released: Vec::new(),
        };
        assert_eq!(
            arena.duplicate_shadow(dst, src, &mut tex).unwrap(),
            SCENE_OK
        );
        // Close-prefix residue: previous dst geometry is gone.
        assert!(arena.shadow_state(dst).unwrap().members.is_empty());
        assert!(arena.shadow_state(dst).unwrap().faces.is_empty());
        let d = arena.shadow_state(dst).unwrap();
        assert!(Arc::ptr_eq(
            d.def.as_ref().unwrap(),
            arena.shadow_state(src).unwrap().def.as_ref().unwrap()
        ));
        assert_eq!(d.reverse_factor, 1.0);
        assert_eq!(d.color, [0.0, 0.5, 1.0]);
        assert_eq!(d.visible_range, 100.0);
    }

    #[test]
    fn duplicate_from_non_shadow_source_keeps_base_only() {
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let dst = arena.create_shadow_frame(&mut visual).unwrap();
        let plain = arena
            .insert(I3dFrameState::model(), None, Vec::new(), Vec::new(), None)
            .unwrap();
        let mut tex = NullTex {
            released: Vec::new(),
        };
        assert_eq!(
            arena.duplicate_shadow(dst, plain, &mut tex).unwrap(),
            SCENE_OK
        );
        // Base copy ran, Close-prefix cleared dst words, derived shadow words
        // untouched (no source state to copy).
        assert!(arena.shadow_state(dst).unwrap().def.is_none());
        assert_eq!(arena.shadow_state(dst).unwrap().shadow_flags, 0);
    }

    #[test]
    fn set_to_collects_named_on_parts_with_base_bias() {
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let shadow = arena.create_shadow_frame(&mut visual).unwrap();
        arena.set_shadow_def(shadow, Some(def_two_parts())).unwrap();
        let root = arena
            .insert(I3dFrameState::model(), None, Vec::new(), Vec::new(), None)
            .unwrap();
        let door = named_child(&mut arena, root, b"door", true);
        let _hood = named_child(&mut arena, root, b"hood", true);
        let wheel = named_child(&mut arena, root, b"wheel", true);
        visual.log.clear();
        arena
            .set_shadow_to(shadow, Some(root), &mut visual)
            .unwrap();
        assert_eq!(visual.log, ["unlink"]);
        let s = arena.shadow_state(shadow).unwrap();
        // Match order follows the child chain (newest sibling first): wheel
        // lands at cursor 0 with bias 0, door at cursor 3 with bias 3.
        assert_eq!(s.faces, [0, 1, 2, 0, 2, 1, 3, 4, 3]);
        assert_eq!(s.verts.len(), 5);
        assert_eq!(s.world_verts.len(), 5);
        assert_eq!(s.points.len(), 5);
        assert_eq!(
            s.members,
            [
                ShadowMemberFrame {
                    frame: Some(wheel),
                    num_verts: 3
                },
                ShadowMemberFrame {
                    frame: Some(door),
                    num_verts: 2
                },
            ]
        );
        assert_eq!(
            arena.state(door).unwrap().flags & SHADOW_MEMBER_FLAG,
            SHADOW_MEMBER_FLAG
        );
        assert_eq!(
            arena.state(wheel).unwrap().flags & SHADOW_MEMBER_FLAG,
            SHADOW_MEMBER_FLAG
        );
    }

    #[test]
    fn set_to_zero_faces_returns_empty_without_residue() {
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let shadow = arena.create_shadow_frame(&mut visual).unwrap();
        arena
            .set_shadow_def(
                shadow,
                Some(Arc::new(ShadowDefData {
                    parts: vec![ShadowPart {
                        name: b"door".to_vec(),
                        num_verts: 2,
                        num_faces: 0,
                    }],
                    verts: vec![[0.0; 3], [0.0; 3]],
                    faces: vec![],
                })),
            )
            .unwrap();
        let root = arena
            .insert(I3dFrameState::model(), None, Vec::new(), Vec::new(), None)
            .unwrap();
        let door = named_child(&mut arena, root, b"door", true);
        arena
            .set_shadow_to(shadow, Some(root), &mut visual)
            .unwrap();
        let s = arena.shadow_state(shadow).unwrap();
        assert!(s.faces.is_empty() && s.members.is_empty());
        assert_eq!(arena.state(door).unwrap().flags & SHADOW_MEMBER_FLAG, 0);
        assert_eq!((s.verts_count, s.lights_count), (-1, -1));
    }

    #[test]
    fn set_to_without_def_keeps_points_buffer() {
        // `SetTo` never frees `points`: with no def installed the teardown
        // runs but the buffer survives (native keeps/leaks it, then allocates
        // new storage only on the collect path).
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let shadow = arena.create_shadow_frame(&mut visual).unwrap();
        arena.shadow_state_mut(shadow).unwrap().points = vec![[1.0, 2.0]];
        let root = arena
            .insert(I3dFrameState::model(), None, Vec::new(), Vec::new(), None)
            .unwrap();
        arena
            .set_shadow_to(shadow, Some(root), &mut visual)
            .unwrap();
        assert_eq!(arena.shadow_state(shadow).unwrap().points, vec![[1.0, 2.0]]);
    }

    #[test]
    fn set_to_oom_clears_all_five_and_reports() {
        // A matching part declaring u32::MAX faces fails the faces reserve
        // deterministically; all five buffers clear with (-1,-1) counts.
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let shadow = arena.create_shadow_frame(&mut visual).unwrap();
        arena
            .set_shadow_def(
                shadow,
                Some(Arc::new(ShadowDefData {
                    parts: vec![ShadowPart {
                        name: b"door".to_vec(),
                        num_verts: 1,
                        num_faces: u32::MAX,
                    }],
                    verts: vec![[0.0; 3]],
                    faces: vec![],
                })),
            )
            .unwrap();
        let root = arena
            .insert(I3dFrameState::model(), None, Vec::new(), Vec::new(), None)
            .unwrap();
        let _door = named_child(&mut arena, root, b"door", true);
        assert_eq!(
            arena.set_shadow_to(shadow, Some(root), &mut visual),
            Err(ModelError::AllocationOverflow)
        );
        let s = arena.shadow_state(shadow).unwrap();
        assert!(s.faces.is_empty() && s.verts.is_empty());
        assert!(s.world_verts.is_empty() && s.points.is_empty());
        assert!(s.members.is_empty());
        assert_eq!((s.verts_count, s.lights_count), (-1, -1));
    }

    #[test]
    fn set_shadow_lock_clears_flags_and_stamps_lights_minus_one() {
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let key = arena.create_shadow_frame(&mut visual).unwrap();
        arena.shadow_state_mut(key).unwrap().shadow_flags = 0xdead;
        arena.shadow_state_mut(key).unwrap().lights_count = 41;
        arena.set_shadow_lock(key, 3).unwrap();
        let s = arena.shadow_state(key).unwrap();
        assert_eq!((s.lock, s.shadow_flags, s.lights_count), (3, 0, -1));
    }

    #[test]
    fn close_tail_and_open_words() {
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let key = arena.create_shadow_frame(&mut visual).unwrap();
        let mut tex = NullTex {
            released: Vec::new(),
        };
        arena.shadow_close(key, &mut tex).unwrap();
        let s = arena.shadow_state(key).unwrap();
        assert_eq!((s.shadow_flags, s.verts_count, s.lights_count), (0, -1, -1));
        assert!(s.texture.is_none() && s.def.is_none());
        // Alloc-null arm: Close-first ran, store nothing, OUTOFMEM word.
        let mut defb = StubDef { mode: 0, code: 0 };
        assert_eq!(
            arena
                .shadow_open(key, b"shd", 0, &mut tex, &mut defb, &mut visual)
                .unwrap(),
            SHADOW_OUT_OF_MEM
        );
        assert!(arena.shadow_state(key).unwrap().def.is_none());
        // LoadShadow passthrough: empty def kept, code returned.
        let mut defb = StubDef {
            mode: 1,
            code: 0x8000_0007,
        };
        assert_eq!(
            arena
                .shadow_open(key, b"shd", 0, &mut tex, &mut defb, &mut visual)
                .unwrap(),
            0x8000_0007
        );
        assert!(arena.shadow_state(key).unwrap().def.is_some());
        // Success arm without a parent: OK, def installed.
        let mut defb = StubDef { mode: 2, code: 0 };
        assert_eq!(
            arena
                .shadow_open(key, b"shd", 0, &mut tex, &mut defb, &mut visual)
                .unwrap(),
            SCENE_OK
        );
        assert!(arena.shadow_state(key).unwrap().def.is_some());
    }

    #[test]
    fn range_nan_keeps_old_but_zeroes_dir_and_flags() {
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let key = arena.create_shadow_frame(&mut visual).unwrap();
        arena.shadow_set_range(key, 8.0).unwrap();
        arena.shadow_state_mut(key).unwrap().shadow_flags = 0xFF;
        arena.shadow_set_range(key, f32::NAN).unwrap();
        let s = arena.shadow_state(key).unwrap();
        assert_eq!(s.range, 8.0);
        assert_eq!(s.light_dir, [0.0; 3]);
        assert_eq!(s.shadow_flags, 0);
        arena.shadow_set_range(key, 0.0).unwrap();
        assert_eq!(arena.shadow_state(key).unwrap().range, 8.0);
        arena.shadow_set_range(key, 4.0).unwrap();
        assert_eq!(arena.shadow_state(key).unwrap().range, 4.0);
    }

    #[test]
    fn max_texture_distance_words_and_effects() {
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let key = arena.create_shadow_frame(&mut visual).unwrap();
        assert_eq!(
            arena.shadow_set_max_texture_distance(key, -1.0).unwrap(),
            SHADOW_INVALID_PARAMS
        );
        assert_eq!(
            arena.shadow_set_max_texture_distance(key, 33.0).unwrap(),
            SHADOW_INVALID_PARAMS
        );
        // NaN passes both ordered gates and stores (reference order).
        assert_eq!(
            arena
                .shadow_set_max_texture_distance(key, f32::NAN)
                .unwrap(),
            SCENE_OK
        );
        assert!(arena
            .shadow_state(key)
            .unwrap()
            .max_texture_distance
            .is_nan());
        assert_eq!(
            arena.shadow_set_max_texture_distance(key, 32.0).unwrap(),
            SCENE_OK
        );
        assert_eq!(
            arena.shadow_set_max_texture_distance(key, 0.0).unwrap(),
            SCENE_OK
        );
        let s = arena.shadow_state(key).unwrap();
        assert_eq!(s.max_texture_distance, 0.0);
        assert_eq!(s.lights_count, -1);
        assert_eq!(s.shadow_flags, 0);
    }

    #[test]
    fn color_clamp_roundtrip_and_nan_limbs() {
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let key = arena.create_shadow_frame(&mut visual).unwrap();
        arena.shadow_set_color(key, [-1.0, 0.5, 2.0]).unwrap();
        assert_eq!(arena.shadow_state(key).unwrap().color, [0.0, 0.5, 1.0]);
        arena
            .shadow_set_color(key, [f32::NAN, -0.0, f32::INFINITY])
            .unwrap();
        let c = arena.shadow_state(key).unwrap().color;
        assert!(c[0].is_nan());
        assert_eq!(c[1], 0.0);
        assert_eq!(c[2], 1.0);
        let s = arena.shadow_state(key).unwrap();
        assert_eq!(s.shadow_flags, 0);
        assert_eq!(s.lights_count, -1);
    }

    #[test]
    fn reverse_raw_first_nan_residue_and_flags() {
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let key = arena.create_shadow_frame(&mut visual).unwrap();
        arena.shadow_set_reverse_factor(key, -2.0).unwrap();
        assert_eq!(arena.shadow_state(key).unwrap().reverse_factor, 0.0);
        arena.shadow_set_reverse_factor(key, 2.0).unwrap();
        assert_eq!(arena.shadow_state(key).unwrap().reverse_factor, 1.0);
        arena.shadow_set_reverse_factor(key, f32::NAN).unwrap();
        assert!(arena.shadow_state(key).unwrap().reverse_factor.is_nan());
        assert_eq!(arena.shadow_state(key).unwrap().shadow_flags, 0);
    }

    #[test]
    fn orthogonal_lights_zero_flags_order() {
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let key = arena.create_shadow_frame(&mut visual).unwrap();
        arena.shadow_set_orthogonal(key, false).unwrap();
        let s = arena.shadow_state(key).unwrap();
        assert!(!s.orthogonal);
        assert_eq!(s.lights_count, -1);
        assert_eq!(s.light_dir, [0.0; 3]);
        assert_eq!(s.shadow_flags, 0);
        arena.shadow_set_orthogonal(key, true).unwrap();
        assert!(arena.shadow_state(key).unwrap().orthogonal);
    }

    #[test]
    fn texture_size_validation_rounding_bug_and_residue() {
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let key = arena.create_shadow_frame(&mut visual).unwrap();
        let mut tex = NullTex {
            released: Vec::new(),
        };
        assert_eq!(
            arena
                .shadow_set_texture_size(key, 129, 4, &mut tex)
                .unwrap(),
            SHADOW_INVALID_PARAMS
        );
        assert_eq!(
            arena
                .shadow_set_texture_size(key, 4, 200, &mut tex)
                .unwrap(),
            SHADOW_INVALID_PARAMS
        );
        assert_eq!(
            (
                arena.shadow_state(key).unwrap().size_x,
                arena.shadow_state(key).unwrap().size_y
            ),
            (64, 64)
        );
        arena
            .shadow_set_texture_size(key, 100, 5, &mut tex)
            .unwrap();
        assert_eq!(
            (
                arena.shadow_state(key).unwrap().size_x,
                arena.shadow_state(key).unwrap().size_y
            ),
            (128, 128)
        );
        arena.shadow_set_texture_size(key, 3, 5, &mut tex).unwrap();
        assert_eq!(
            (
                arena.shadow_state(key).unwrap().size_x,
                arena.shadow_state(key).unwrap().size_y
            ),
            (4, 4)
        );
        arena.shadow_set_texture_size(key, 0, 0, &mut tex).unwrap();
        assert_eq!(
            (
                arena.shadow_state(key).unwrap().size_x,
                arena.shadow_state(key).unwrap().size_y
            ),
            (4, 4)
        );
        let s = arena.shadow_state(key).unwrap();
        assert_eq!(s.lights_count, -1);
        assert_eq!(s.shadow_flags, 0);
        // Creation happens in RenderTexture, never here: no texture installed,
        // no release ran (the trait no longer even offers a create method).
        assert!(arena.shadow_state(key).unwrap().texture.is_none());
        assert!(tex.released.is_empty());
    }

    #[test]
    fn update_v_joins_first_sector_scene_and_resets_caches() {
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sector = arena.scene(scene).unwrap().sectors[0];
        let shadow = arena.create_shadow_frame(&mut visual).unwrap();
        arena
            .link_frame_base(shadow, Some(sector.frame()), 0)
            .unwrap();
        arena.update_shadow_v(shadow).unwrap();
        assert_eq!(arena.shadow_state(shadow).unwrap().scene, Some(scene));
        assert_eq!(arena.scene(scene).unwrap().shadows, [shadow]);
        // Second call is the scene-present early-out (no duplicate).
        arena.update_shadow_v(shadow).unwrap();
        assert_eq!(arena.scene(scene).unwrap().shadows, [shadow]);
        // Detach via null-target SetTo clears registry and back-pointer.
        arena.set_shadow_to(shadow, None, &mut visual).unwrap();
        assert!(arena.scene(scene).unwrap().shadows.is_empty());
        assert!(arena.shadow_state(shadow).unwrap().scene.is_none());
    }

    #[test]
    fn delete_from_any_shadow_nulls_every_membership_and_clears_flag() {
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let a = arena.create_shadow_frame(&mut visual).unwrap();
        let b = arena.create_shadow_frame(&mut visual).unwrap();
        arena.set_shadow_def(a, Some(def_two_parts())).unwrap();
        arena.set_shadow_def(b, Some(def_two_parts())).unwrap();
        let root = arena
            .insert(I3dFrameState::model(), None, Vec::new(), Vec::new(), None)
            .unwrap();
        let door = named_child(&mut arena, root, b"door", true);
        arena.set_shadow_to(a, Some(root), &mut visual).unwrap();
        arena.set_shadow_to(b, Some(root), &mut visual).unwrap();
        arena.delete_frame_from_any_shadow(door).unwrap();
        for shadow in [a, b] {
            assert!(arena
                .shadow_state(shadow)
                .unwrap()
                .members
                .iter()
                .all(|m| m.frame.is_none()));
        }
        assert_eq!(arena.state(door).unwrap().flags & SHADOW_MEMBER_FLAG, 0);
    }

    #[test]
    fn driver_delete_from_all_shadows_forwards_to_any_shadow() {
        // Driver 0x1a0e0 tail-call parity: forwarder behaves exactly like
        // the callee (member nulled + flag cleared; non-member no-op Ok).
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let a = arena.create_shadow_frame(&mut visual).unwrap();
        arena.set_shadow_def(a, Some(def_two_parts())).unwrap();
        let root = arena
            .insert(I3dFrameState::model(), None, Vec::new(), Vec::new(), None)
            .unwrap();
        let door = named_child(&mut arena, root, b"door", true);
        arena.set_shadow_to(a, Some(root), &mut visual).unwrap();
        arena.driver_delete_from_all_shadows(door).unwrap();
        assert!(arena
            .shadow_state(a)
            .unwrap()
            .members
            .iter()
            .all(|m| m.frame != Some(door)));
        arena.driver_delete_from_all_shadows(door).unwrap();
    }

    #[test]
    fn driver_delete_from_all_shadows_tail_call_is_surgical_and_idempotent() {
        // Driver 0x1a0e0 (mafia-re I3d_driver.cpp:1754-1757) tail-calls
        // DeleteFromAnyShadow: a non-member frame is a no-op Ok, removal
        // nulls only the matching slots, and a second call is a no-op.
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let a = arena.create_shadow_frame(&mut visual).unwrap();
        let b = arena.create_shadow_frame(&mut visual).unwrap();
        arena.set_shadow_def(a, Some(def_two_parts())).unwrap();
        arena.set_shadow_def(b, Some(def_two_parts())).unwrap();
        let root = arena
            .insert(I3dFrameState::model(), None, Vec::new(), Vec::new(), None)
            .unwrap();
        let door = named_child(&mut arena, root, b"door", true);
        let other = named_child(&mut arena, root, b"other", true);
        arena.set_shadow_to(a, Some(root), &mut visual).unwrap();
        arena.set_shadow_to(b, Some(root), &mut visual).unwrap();
        // Non-member: no-op success, matching slots and flags untouched.
        arena.delete_frame_from_any_shadow(other).unwrap();
        for shadow in [a, b] {
            assert!(arena
                .shadow_state(shadow)
                .unwrap()
                .members
                .iter()
                .any(|m| m.frame == Some(door)));
        }
        assert_ne!(arena.state(door).unwrap().flags & SHADOW_MEMBER_FLAG, 0);
        // Member removal, then idempotent repeat.
        arena.delete_frame_from_any_shadow(door).unwrap();
        assert_eq!(arena.state(door).unwrap().flags & SHADOW_MEMBER_FLAG, 0);
        arena.delete_frame_from_any_shadow(door).unwrap();
        arena.delete_frame_from_any_shadow(other).unwrap();
        for shadow in [a, b] {
            assert!(arena
                .shadow_state(shadow)
                .unwrap()
                .members
                .iter()
                .all(|m| m.frame.is_none()));
        }
    }

    #[test]
    fn link_shadow_runs_set_to_before_base() {
        let mut arena = ModelArena::default();
        let mut visual = NullVisual { log: Vec::new() };
        let shadow = arena.create_shadow_frame(&mut visual).unwrap();
        arena.set_shadow_def(shadow, Some(def_two_parts())).unwrap();
        let root = arena
            .insert(I3dFrameState::model(), None, Vec::new(), Vec::new(), None)
            .unwrap();
        let door = named_child(&mut arena, root, b"door", true);
        arena
            .link_shadow_frame(shadow, Some(root), 0, &mut visual)
            .unwrap();
        assert_eq!(arena.links(shadow).unwrap().parent, Some(root));
        assert_eq!(
            arena.state(door).unwrap().flags & SHADOW_MEMBER_FLAG,
            SHADOW_MEMBER_FLAG
        );
    }
}
