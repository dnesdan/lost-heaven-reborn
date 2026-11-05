//! I3D particle emitter (I3D_emitor) + particle-renderer state (PC LS3DF.dll
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6,
//! GROUP Y1: 8 differential + 6 contract).
//!
//! Upstream working names (mafia-re ls3df/I3D/visuals/I3d_particle.cpp,
//! I3D.h), not evidenced original symbols; binary/RVA mapping retained per
//! item. No FrameKind::Emitor exists (FrameKind checked); emitor frames use
//! the neutral Model carrier and are discriminated by `LoadedFrame.emitor`
//! (same pattern as projector/mirror/shadow). I3D_part_element has no arena
//! home yet, so elements are backend-owned FrameKeys.
//! Precision policy: all param floats are f32 with bitwise u32 transfer
//! (from_bits/to_bits); formula order is native source order with host f32
//! ops (no fused ops, no clamping); x87 80-bit intermediate deviation is
//! quarantined per item and needs original-runtime validation. Truncation
//! toward zero mirrors (uint) chop; out-of-range saturation is a disclosed
//! hardening deviation (native wraps). NaN is preserved by the manual clamp
//! limbs and refused at pack boundaries. Device calls (operator new, VB/IB
//! alloc/lock/unlock, every D3D8 state/draw call, RandFloat, dbgPrintf,
//! matrix/vector math) stay backend contracts. The native element sort
//! primitive is a caller contract (algorithm unevidenced). Helper phases and
//! pure planners get no function credit.
use crate::frame_transform::{FrameKey, FrameKind, I3dFrameState};
use crate::model_loading::{ModelArena, ModelError};
use crate::native_transform::SMatrix;
use crate::scene_ownership::SectorKey;

/// LS3D_RESULT words for the emitter paths. 0x80000007 is corroborated at
/// mirror RVA 0x31ab0 (MOV EAX imm); re-pin for particle paths (OPEN).
pub const EMITOR_OK: u32 = 0;
pub const EMITOR_INVALIDPARAMS: u32 = 0x8000_0007;
/// m_Type word stored by the ctor at +0x110 (Ghidra 0x3de50).
pub const FRAME_EMITOR_TYPE: u32 = 0x11;
/// m_FrmFlags words. ON from I3D.h:259 (upstream); UPDATE|CHANGED combined
/// word evidenced by TEST [ECX+0xac],0x40002000 at 0x3eb10 (split
/// 0x40000000|0x2000 per I3D.h:268/287, upstream).
pub const FRMFLAGS_ON: u32 = 0x1;
pub const FRMFLAGS_UPDATE_CHANGED: u32 = 0x4000_2000;
/// part_flags words (I3D.h:3532-3540, names upstream; MESH 0x200
/// corroborated by Ghidra 0x3d0e0 `& 0x200`; 0x104 store at 0x3e930
/// corroborates UNKNOWN_100|UNKNOWN_4).
pub const PARTF_OPAQUE: u32 = 0x1;
pub const PARTF_ALPHA: u32 = 0x2;
pub const PARTF_ALPHA_MASK: u32 = 0x3;
pub const PARTF_UNKNOWN_4: u32 = 0x4;
pub const PARTF_UNKNOWN_100: u32 = 0x100;
pub const PARTF_MESH: u32 = 0x200;
pub const PARTF_RENDERER: u32 = 0x600;
/// Sort-key words (I3D.h:4578-4581, upstream) and texture flag (I3D.h:3855).
pub const PRIM_SORT_ALPHA: u32 = 0x8000_0000;
pub const PRIM_SORT_DIST_MASK: u32 = 0x3fff_ffff;
pub const PRIM_SORT_MAT_MASK: u32 = 0x7fff;
pub const MATF_TEXTURE: u32 = 0x40000;
pub const VISF_COLOR_CHANGED: u32 = 0x4;
/// Renderer buffer sizes (I3d_particle.cpp:32-34, upstream): all four
/// corroborated by the Ghidra 0x3d0e0 body (VB 0x2000, IB 0x3000, UV loop
/// 0x800, index loop 0x800).
pub const PART_RENDER_QUADS: u32 = 0x800;
pub const PART_RENDER_VERTS: u32 = 0x2000;
pub const PART_RENDER_INDICES: u32 = 0x3000;

/// Emitter parameter selector. Discriminants from the PARTICLE_PARAM enum
/// (I3D.h:2115-2150, order NOT the SetParam switch order); PP_LAST=34 is
/// corroborated by CMP EAX,0x22 at both 0x3dff0 and 0x3e660 entries. Names
/// are upstream working names, not original symbols.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ParticleParam {
    EmitTime = 0,
    EmitTimeRnd = 1,
    LifeTime = 2,
    LifeTimeRnd = 3,
    Grow1 = 4,
    Grow2 = 5,
    PosRnd = 6,
    DirRnd = 7,
    DirTime = 8,
    DirTimeRnd = 9,
    DirTimeStray = 10,
    FadeIn = 11,
    FadeOut = 12,
    VelocityX = 13,
    VelocityY = 14,
    VelocityZ = 15,
    VelocityEndX = 16,
    VelocityEndY = 17,
    VelocityEndZ = 18,
    Flags = 19,
    MaxParticles = 20,
    VelocityWorld = 21,
    VelocityEndWorld = 22,
    PreTick = 23,
    RotSpeed1 = 24,
    RotSpeed2 = 25,
    Scale = 26,
    Unused27 = 27,
    Billboard = 28,
    Color = 29,
    ColorEnd = 30,
    OmniSector = 31,
    RotAxis = 32,
    AxisMode = 33,
}
pub const PARTICLE_PARAM_LAST: u32 = 34;
impl ParticleParam {
    pub fn from_u32(v: u32) -> Option<Self> {
        const TABLE: [ParticleParam; 34] = [
            ParticleParam::EmitTime,
            ParticleParam::EmitTimeRnd,
            ParticleParam::LifeTime,
            ParticleParam::LifeTimeRnd,
            ParticleParam::Grow1,
            ParticleParam::Grow2,
            ParticleParam::PosRnd,
            ParticleParam::DirRnd,
            ParticleParam::DirTime,
            ParticleParam::DirTimeRnd,
            ParticleParam::DirTimeStray,
            ParticleParam::FadeIn,
            ParticleParam::FadeOut,
            ParticleParam::VelocityX,
            ParticleParam::VelocityY,
            ParticleParam::VelocityZ,
            ParticleParam::VelocityEndX,
            ParticleParam::VelocityEndY,
            ParticleParam::VelocityEndZ,
            ParticleParam::Flags,
            ParticleParam::MaxParticles,
            ParticleParam::VelocityWorld,
            ParticleParam::VelocityEndWorld,
            ParticleParam::PreTick,
            ParticleParam::RotSpeed1,
            ParticleParam::RotSpeed2,
            ParticleParam::Scale,
            ParticleParam::Unused27,
            ParticleParam::Billboard,
            ParticleParam::Color,
            ParticleParam::ColorEnd,
            ParticleParam::OmniSector,
            ParticleParam::RotAxis,
            ParticleParam::AxisMode,
        ];
        (v < PARTICLE_PARAM_LAST).then(|| TABLE[v as usize])
    }
}

/// Opaque host handle for the emitter mesh (native I3D_mesh_object* with
/// AddRef/Release; pointer values never cross into the host).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmitorMeshHandle(pub u32);

/// Full emitter parameter + runtime state (Reset defaults are
/// emitor_reset_defaults; differential at 0x3e930).
#[derive(Clone, Debug, PartialEq)]
pub struct I3dEmitorState {
    pub emit_time_ms: u32,
    pub emit_time_rnd: u32,
    pub life_time: u32,
    pub life_time_rnd: u32,
    pub scale: f32,
    pub grow_1: f32,
    pub grow_2: f32,
    pub pos_rnd: f32,
    pub dir_rnd: f32,
    pub fade_in: f32,
    pub fade_out: f32,
    pub dir_time: u32,
    pub dir_time_rnd: u32,
    pub dir_time_stray: f32,
    pub velocity: [f32; 3],
    pub velocity_end: [f32; 3],
    pub max_particles: u32,
    pub velocity_world: bool,
    pub velocity_end_world: bool,
    pub rot_speed_1: f32,
    pub rot_speed_2: f32,
    pub pre_tick: u32,
    pub billboard: bool,
    pub rot_axis: u32,
    pub axis_mode: u32,
    pub color: [f32; 3],
    pub color_end: [f32; 3],
    pub omni_sector: bool,
    pub part_flags: u32,
    pub mesh: Option<EmitorMeshHandle>,
    pub time_ms: u32,
    pub emit_countdown: u32,
    pub num_emitted: u32,
    pub sector: Option<SectorKey>,
    pub id: u16,
    pub elements: Vec<FrameKey>,
}

impl I3dEmitorState {
    /// Original Reset, RVA 0x3e930 (differential: 66 insn/1 BB, 28 stores).
    /// Constants are the exact store immediates (floats via bits).
    pub(crate) fn reset_defaults() -> Self {
        Self {
            emit_time_ms: 100,
            emit_time_rnd: 60,
            life_time: 1800,
            life_time_rnd: 600,
            scale: f32::from_bits(0x3f80_0000),
            grow_1: f32::from_bits(0xbe80_0000),
            grow_2: f32::from_bits(0x3f19_999a),
            pos_rnd: 0.0,
            dir_rnd: 0.0,
            fade_in: f32::from_bits(0x3dcc_cccd),
            fade_out: f32::from_bits(0x3dcc_cccd),
            dir_time: 0,
            dir_time_rnd: 0,
            dir_time_stray: f32::from_bits(0x3dcc_cccd),
            velocity: [0.0, f32::from_bits(0x4000_0000), 0.0],
            velocity_end: [f32::from_bits(0x4000_0000), 0.0, 0.0],
            max_particles: 0,
            velocity_world: false,
            velocity_end_world: false,
            rot_speed_1: f32::from_bits(0xbfa0_d97c),
            rot_speed_2: f32::from_bits(0x3fa0_d97c),
            pre_tick: 0,
            billboard: true,
            rot_axis: 0,
            axis_mode: 3,
            color: [1.0, 1.0, 1.0],
            color_end: [1.0, 1.0, 1.0],
            omni_sector: false,
            part_flags: PARTF_UNKNOWN_100 | PARTF_UNKNOWN_4,
            mesh: None,
            time_ms: 0,
            emit_countdown: 0,
            num_emitted: 0,
            sector: None,
            id: 0,
            elements: Vec::new(),
        }
    }
}

/// Native (uint)(f32) chop. Host `as` saturates; native FISTP wraps — the
/// saturation is a disclosed hardening deviation, bit-exact in range.
fn trunc_u32(v: f32) -> u32 {
    v.trunc() as u32
}
/// Recompute word `(RandFloat()*rnd + base)` (SetParam EMIT_TIME_RND arm,
/// Tick tail). f32 mul-then-add in source order; x87-temp deviation OPEN.
fn recompute_emit_time(base: u32, rnd: u32, sample: f32) -> u32 {
    trunc_u32(sample * rnd as f32 + base as f32)
}

/// RNG + re-entrant Tick + color-pointer resolution for SetParam. RandFloat
/// range ([0,1) per upstream use) is a backend contract.
pub trait EmitorParamBackend {
    fn rand_float(&mut self) -> f32;
    fn tick_emitor_for(&mut self, key: FrameKey, ms: u32) -> Result<(), ModelError>;
    fn resolve_param_color(&self, word: u32) -> Option<[f32; 3]>;
    fn alloc_emitor_id(&mut self) -> u16;
}
/// Mesh AddRef/Release pair for SetMesh/dtor (device/host resource contract).
pub trait EmitorMeshBackend {
    fn retain_mesh(&mut self, mesh: EmitorMeshHandle);
    fn release_mesh(&mut self, mesh: EmitorMeshHandle);
}

/// Manual [0,1] clamp limbs (SetParam COLOR arms). NaN fails both compares
/// and is preserved, matching x87 FCOMP/JP/JNZ order. Helper: no credit.
#[allow(clippy::manual_clamp)]
fn clamp_color_in_place(color: &mut [f32; 3]) {
    for lane in color.iter_mut() {
        if *lane < 0.0 {
            *lane = 0.0;
        } else if *lane > 1.0 {
            *lane = 1.0;
        }
    }
}

/// Original SetParam, RVA 0x3dff0 (contract: no Ghidra body; entry guard
/// CMP 0x22 + vtable slot 0x1009c58c). Arms in native switch order with
/// exact effect order; failure word leaves prior stores retained (only the
/// guard and null-color paths fail, both before their stores... except
/// PRE_TICK, whose store precedes the backend Tick like natively).
pub fn emitor_set_param(
    state: &mut I3dEmitorState,
    key: FrameKey,
    param: u32,
    value: u32,
    backend: &mut impl EmitorParamBackend,
) -> u32 {
    let Some(selector) = ParticleParam::from_u32(param) else {
        return EMITOR_INVALIDPARAMS;
    };
    match selector {
        ParticleParam::EmitTime => {
            state.emit_time_ms = value;
            state.emit_countdown = 0;
        }
        ParticleParam::EmitTimeRnd => {
            state.emit_time_rnd = value;
            let sample = backend.rand_float();
            state.emit_countdown =
                recompute_emit_time(state.emit_time_ms, state.emit_time_rnd, sample);
        }
        ParticleParam::LifeTime => state.life_time = value,
        ParticleParam::LifeTimeRnd => state.life_time_rnd = value,
        ParticleParam::Scale => state.scale = f32::from_bits(value),
        ParticleParam::Grow1 => state.grow_1 = f32::from_bits(value),
        ParticleParam::Grow2 => state.grow_2 = f32::from_bits(value),
        ParticleParam::PosRnd => state.pos_rnd = f32::from_bits(value),
        ParticleParam::DirRnd => state.dir_rnd = f32::from_bits(value),
        ParticleParam::FadeIn => state.fade_in = f32::from_bits(value),
        ParticleParam::FadeOut => state.fade_out = f32::from_bits(value),
        ParticleParam::DirTime => state.dir_time = value,
        ParticleParam::DirTimeRnd => state.dir_time_rnd = value,
        ParticleParam::DirTimeStray => state.dir_time_stray = f32::from_bits(value),
        ParticleParam::VelocityX => state.velocity[0] = f32::from_bits(value),
        ParticleParam::VelocityY => state.velocity[1] = f32::from_bits(value),
        ParticleParam::VelocityZ => state.velocity[2] = f32::from_bits(value),
        ParticleParam::VelocityEndX => state.velocity_end[0] = f32::from_bits(value),
        ParticleParam::VelocityEndY => state.velocity_end[1] = f32::from_bits(value),
        ParticleParam::VelocityEndZ => state.velocity_end[2] = f32::from_bits(value),
        ParticleParam::MaxParticles => {
            state.max_particles = value;
            // Counter reset + ON-bit are the composer's SetOn(true) (native
            // SetParam calls SetOn after the store); the pure arm stores only.
        }
        ParticleParam::VelocityWorld => state.velocity_world = value != 0,
        ParticleParam::VelocityEndWorld => state.velocity_end_world = value != 0,
        ParticleParam::RotSpeed1 => state.rot_speed_1 = f32::from_bits(value),
        ParticleParam::RotSpeed2 => state.rot_speed_2 = f32::from_bits(value),
        ParticleParam::PreTick => {
            state.pre_tick = value;
            if value != 0 {
                // Native Tick(value) runs with the store retained; a backend
                // failure propagates the same way (no recovery branch).
                if backend.tick_emitor_for(key, value).is_err() {
                    return EMITOR_INVALIDPARAMS;
                }
            }
        }
        ParticleParam::Billboard => state.billboard = value != 0,
        ParticleParam::RotAxis => state.rot_axis = value,
        ParticleParam::AxisMode => state.axis_mode = value,
        ParticleParam::Color => {
            let Some(rgb) = backend.resolve_param_color(value) else {
                return EMITOR_INVALIDPARAMS;
            };
            state.color = rgb;
            clamp_color_in_place(&mut state.color);
        }
        ParticleParam::ColorEnd => {
            let Some(rgb) = backend.resolve_param_color(value) else {
                return EMITOR_INVALIDPARAMS;
            };
            state.color_end = rgb;
            clamp_color_in_place(&mut state.color_end);
        }
        ParticleParam::OmniSector => state.omni_sector = value != 0,
        ParticleParam::Flags => state.part_flags = value,
        // PP_UNUSED_27: no SetParam arm (falls out of the switch, I3D_OK).
        ParticleParam::Unused27 => {}
    }
    EMITOR_OK
}

/// Pure ON-bit word op for the SetOn paths (SetOn body + SetParam
/// MAX_PARTICLES arm). Helper: no function credit.
pub fn apply_emitor_set_on(flags: u32, on: bool) -> (u32, bool) {
    if on {
        (flags | FRMFLAGS_ON, true)
    } else {
        (flags & !FRMFLAGS_ON, false)
    }
}

/// Original GetParam, RVA 0x3e660 (contract; same guard evidence as
/// SetParam). Floats as bits, byte params masked (I3DByteAsInt), UNUSED_27
/// aliases BILLBOARD, colors are caller-supplied opaque words standing in
/// for the native `&p_color`/`&p_color_end` pointer returns.
pub fn emitor_get_param(
    state: &I3dEmitorState,
    param: u32,
    color_word: u32,
    color_end_word: u32,
) -> u32 {
    let Some(selector) = ParticleParam::from_u32(param) else {
        return 0;
    };
    match selector {
        ParticleParam::EmitTime => state.emit_time_ms,
        ParticleParam::EmitTimeRnd => state.emit_time_rnd,
        ParticleParam::LifeTime => state.life_time,
        ParticleParam::LifeTimeRnd => state.life_time_rnd,
        ParticleParam::Scale => state.scale.to_bits(),
        ParticleParam::Grow1 => state.grow_1.to_bits(),
        ParticleParam::Grow2 => state.grow_2.to_bits(),
        ParticleParam::PosRnd => state.pos_rnd.to_bits(),
        ParticleParam::DirRnd => state.dir_rnd.to_bits(),
        ParticleParam::FadeIn => state.fade_in.to_bits(),
        ParticleParam::FadeOut => state.fade_out.to_bits(),
        ParticleParam::DirTime => state.dir_time,
        ParticleParam::DirTimeRnd => state.dir_time_rnd,
        ParticleParam::DirTimeStray => state.dir_time_stray.to_bits(),
        ParticleParam::VelocityX => state.velocity[0].to_bits(),
        ParticleParam::VelocityY => state.velocity[1].to_bits(),
        ParticleParam::VelocityZ => state.velocity[2].to_bits(),
        ParticleParam::VelocityEndX => state.velocity_end[0].to_bits(),
        ParticleParam::VelocityEndY => state.velocity_end[1].to_bits(),
        ParticleParam::VelocityEndZ => state.velocity_end[2].to_bits(),
        ParticleParam::MaxParticles => state.max_particles,
        ParticleParam::VelocityWorld => u32::from(state.velocity_world),
        ParticleParam::VelocityEndWorld => u32::from(state.velocity_end_world),
        ParticleParam::RotSpeed1 => state.rot_speed_1.to_bits(),
        ParticleParam::RotSpeed2 => state.rot_speed_2.to_bits(),
        ParticleParam::PreTick => state.pre_tick,
        ParticleParam::Unused27 | ParticleParam::Billboard => u32::from(state.billboard),
        ParticleParam::RotAxis => state.rot_axis,
        ParticleParam::AxisMode => state.axis_mode,
        ParticleParam::Color => color_word,
        ParticleParam::ColorEnd => color_end_word,
        ParticleParam::OmniSector => u32::from(state.omni_sector),
        ParticleParam::Flags => state.part_flags,
    }
}

/// Original UpdateV predicate, RVA 0x3eb10 (contract: TEST [ECX+0xac],
/// 0x40002000). True when the sector must be dropped. Pure.
pub fn emitor_update_v(flags: u32) -> bool {
    flags & FRMFLAGS_UPDATE_CHANGED != 0
}

/// Late-tick word `time - emit_base` (Tick tail: only when time >
/// emit_time, then gated on late > 0). Native computes signed
/// `int late = time - emit_base` and re-tests `late > 0`; the host replicates
/// both gates on the u32 words (0 unless time beats BOTH bounds), so an
/// out-of-contract sample can never yield a gigantic wrapped `late_ms`.
/// Pure. Helper.
pub fn emitor_late_ms(time_ms: u32, emit_base: u32, emit_time: u32) -> u32 {
    if time_ms > emit_time && time_ms > emit_base {
        time_ms.wrapping_sub(emit_base)
    } else {
        0
    }
}

/// Ordered RNG samples for one emission. Draw order is the native call order
/// (I3d_particle.cpp:712-765): position jitter ×3, direction jitter ×3, then
/// life, dir_period, rot, grow, emit — 11 draws per emission. The driver
/// draws all 11 upfront in this stream order; native interleaves backend
/// effects (SetPos, LinkTo, scene lists, matrix reads) between the draw
/// groups, which the host does not reproduce: the emission backend must not
/// consume emission RNG mid-call, or the shared stream desyncs (disclosed).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EmitorRandSamples {
    pub pos_jitter: [f32; 3],
    pub dir_jitter: [f32; 3],
    pub life: f32,
    pub dir_period: f32,
    pub rot: f32,
    pub grow: f32,
    pub emit: f32,
}
/// Pure derived emission words (Tick :748-765) over caller-supplied
/// samples. None on refused (non-finite) samples — disclosed hardening.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EmitorDerived {
    pub life: u32,
    pub life_max: u32,
    /// Native `(int)(RandFloat()*p_dir_time_rnd + p_dir_time)` (signed).
    pub dir_period: i32,
    pub fade_in: u32,
    pub fade_out: u32,
    pub rot_speed: f32,
    pub grow: f32,
    pub emit_time: u32,
}
pub fn emitor_derived_params(
    life_base: u32,
    life_rnd: u32,
    fade_in_frac: f32,
    fade_out_frac: f32,
    rot_1: f32,
    rot_2: f32,
    grow_1: f32,
    grow_2: f32,
    emit_base: u32,
    emit_rnd: u32,
    dir_time: u32,
    dir_time_rnd: u32,
    s: &EmitorRandSamples,
) -> Option<EmitorDerived> {
    for sample in [
        s.life,
        s.dir_period,
        s.rot,
        s.grow,
        s.emit,
        s.pos_jitter[0],
        s.pos_jitter[1],
        s.pos_jitter[2],
        s.dir_jitter[0],
        s.dir_jitter[1],
        s.dir_jitter[2],
    ] {
        if !sample.is_finite() {
            return None;
        }
    }
    let life = trunc_u32(s.life * life_rnd as f32 + life_base as f32);
    let life_f = life as f32;
    let grow_min = grow_1.min(grow_2);
    let grow_max = grow_1.max(grow_2);
    // Bit-preserving i32 reinterpret of the truncated word (exact in the
    // common range; native out-of-range float-to-int is undefined, host
    // saturating `as` is the disclosed hardening there).
    let dir_period = trunc_u32(s.dir_period * dir_time_rnd as f32 + dir_time as f32) as i32;
    Some(EmitorDerived {
        life,
        life_max: life_rnd.wrapping_add(life_base),
        dir_period,
        fade_in: trunc_u32(life_f * fade_in_frac),
        fade_out: trunc_u32(life_f - life_f * fade_out_frac),
        rot_speed: (s.rot * (rot_2 - rot_1) + rot_1) * 0.001,
        grow: (s.grow * (grow_max - grow_min) + grow_min) * 0.001,
        emit_time: recompute_emit_time(emit_base, emit_rnd, s.emit),
    })
}

/// Single due-emission accounting step for the Tick loop (RVA 0x3eb30,
/// contract). Native order: time -= emit_time, num_emitted++,
/// max-cutoff → SetOn(false) + stop. Pure over live-reread words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmitorTickStep {
    /// Emit one particle; carries the post-decrement time word.
    Emit { time_left: u32 },
    /// Max-particles cutoff fired. Native decrements time and increments the
    /// counter BEFORE the cutoff test (I3d_particle.cpp:674-680), so the
    /// carried words must be stored before SetOn(false); the loop stops with
    /// no emission.
    SwitchedOff { time_left: u32, emitted: u32 },
    /// Nothing due.
    Wait,
}
pub fn emitor_tick_step(
    on: bool,
    time_ms: u32,
    emit_countdown: u32,
    num_emitted: u32,
    max_particles: u32,
) -> EmitorTickStep {
    if !on || time_ms < emit_countdown {
        return EmitorTickStep::Wait;
    }
    let time_left = time_ms.wrapping_sub(emit_countdown);
    let emitted = num_emitted.wrapping_add(1);
    if max_particles != 0 && emitted > max_particles {
        return EmitorTickStep::SwitchedOff { time_left, emitted };
    }
    EmitorTickStep::Emit { time_left }
}

/// Per-emission views handed to the element backend. The emitter
/// scale/pos/matrix words below are backend-supplied live views (native
/// reads GetScale/GetWorldPos/GetMatrix there); identity values here are
/// placeholders the backend must overwrite — never a claimed default.
pub struct EmitorEmissionInput {
    pub sector: SectorKey,
    pub mesh: Option<EmitorMeshHandle>,
    pub billboard: bool,
    pub rot_axis: u32,
    pub axis_mode: u32,
    pub own_part_flags: u32,
    pub emitor_id: u16,
    pub emitter_scale: [f32; 3],
    pub emitter_pos: [f32; 3],
    pub emitter_matrix: SMatrix,
    pub scale: f32,
    pub pos_rnd: f32,
    pub dir_rnd: f32,
    pub velocity: [f32; 3],
    pub velocity_end: [f32; 3],
    pub velocity_world: bool,
    pub velocity_end_world: bool,
    pub color: [f32; 3],
    pub color_end: [f32; 3],
    pub derived: EmitorDerived,
    pub samples: EmitorRandSamples,
    /// Newborn late-tick (`emitor_late_ms`): the backend must Tick the emitted
    /// element for this long AFTER init when nonzero (native tail order:
    /// `emit_time` store first, then `e->Tick(late)`).
    pub late_ms: u32,
}
pub trait EmitorEmissionBackend: EmitorParamBackend {
    fn emit_particle(
        &mut self,
        emitor: FrameKey,
        input: &EmitorEmissionInput,
    ) -> Result<(), ModelError>;
}

/// One queued particle's m_AddFGroups inputs (caller = render-queue walk).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParticleSortEntry {
    pub list_index: usize,
    pub sort_group: u16,
    pub sort_dist: f32,
    pub part_flags: u32,
    pub fade: f32,
    pub transparency: f32,
    pub texture_uid: Option<u32>,
    pub has_sector: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParticleFaceGroup {
    /// Top sorted position (`h.first = j`): indexes the caller's SORTED
    /// array, not the original visual list. The renderer walks `sort[first]`
    /// down through `num` entries; `list_index` words stay on the entries.
    pub first: usize,
    pub num: usize,
    pub sector_from_entry: bool,
    pub texture_uid: Option<u32>,
    pub sort_value: u32,
    pub opaque: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParticleFgroupPlan {
    pub groups: Vec<ParticleFaceGroup>,
    pub num_opaque: usize,
    pub num_transparent: usize,
}
/// Original m_AddFGroups grouping, RVA 0x3cd40 (contract). Input must
/// already be sorted by sort_value ascending and is consumed from the top
/// (native `j` walks down); the native sort primitive is a caller contract.
/// Empty input yields no groups (native early return).
pub fn particle_fgroup_plan(sorted_desc: &[ParticleSortEntry]) -> ParticleFgroupPlan {
    let mut plan = ParticleFgroupPlan {
        groups: Vec::new(),
        num_opaque: 0,
        num_transparent: 0,
    };
    if sorted_desc.is_empty() {
        return plan;
    }
    let mut j = sorted_desc.len();
    loop {
        j -= 1;
        let top = &sorted_desc[j];
        let first = j;
        let mut num = 1usize;
        if j > 0 {
            let group = top.sort_group;
            let mut dist = top.sort_dist;
            while j > 0 && sorted_desc[j - 1].sort_group == group {
                j -= 1;
                dist += sorted_desc[j].sort_dist;
                num += 1;
            }
            plan.groups.push(ParticleFaceGroup {
                first,
                num,
                sector_from_entry: top.has_sector,
                texture_uid: top.texture_uid,
                sort_value: ((trunc_u32(dist) / num as u32) & PRIM_SORT_DIST_MASK)
                    | PRIM_SORT_ALPHA,
                opaque: false,
            });
            plan.num_transparent += 1;
            if j == 0 {
                break;
            }
        } else {
            // Singleton tail: opaque iff PARTF_OPAQUE and not faded (NaN
            // product fails >= and takes the transparent arm, as natively).
            #[expect(
                clippy::neg_cmp_op_on_partial_ord,
                reason = "ordered negation keeps NaN faded (transparent); < 1.0 would mark NaN unfaded, flipping an opaque-flagged singleton to opaque"
            )]
            let faded = !(top.fade * top.transparency >= 1.0);
            if !faded && top.part_flags & PARTF_OPAQUE != 0 {
                plan.groups.push(ParticleFaceGroup {
                    first,
                    num,
                    sector_from_entry: top.has_sector,
                    texture_uid: top.texture_uid,
                    sort_value: top
                        .texture_uid
                        .map(|uid| uid & PRIM_SORT_MAT_MASK)
                        .unwrap_or(0),
                    opaque: true,
                });
                plan.num_opaque += 1;
            } else {
                plan.groups.push(ParticleFaceGroup {
                    first,
                    num,
                    sector_from_entry: top.has_sector,
                    texture_uid: top.texture_uid,
                    sort_value: (trunc_u32(top.sort_dist) & PRIM_SORT_DIST_MASK) | PRIM_SORT_ALPHA,
                    opaque: false,
                });
                plan.num_transparent += 1;
            }
            break;
        }
    }
    plan
}

/// PART_COLOR pack (I3d_particle.cpp:158-161): B,G,R,A order, FloatToInt
/// chop each lane. None on refused lanes — non-finite OR finite out-of-byte
/// (>0xff, e.g. fade 2.0): hardening, disclosed (native packs the wrapped
/// word). Negative lanes saturate to 0 via the host `as` cast (native
/// `(int)(-2.0)` = -2 bleeds via OR) — disclosed hardening, same class as
/// `trunc_u32` elsewhere.
pub fn part_color_pack(
    light_mult: f32,
    color: [f32; 3],
    fade: f32,
    transparency: f32,
) -> Option<u32> {
    for lane in [light_mult, color[0], color[1], color[2], fade, transparency] {
        if !lane.is_finite() {
            return None;
        }
    }
    let b = trunc_u32(light_mult * color[2]);
    let g = trunc_u32(light_mult * color[1]);
    let r = trunc_u32(light_mult * color[0]);
    let a = trunc_u32(fade * transparency * 255.0);
    if b > 0xff || g > 0xff || r > 0xff || a > 0xff {
        return None;
    }
    Some(b | (g << 8) | (r << 16) | (a << 24))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParticleMaterialView {
    pub flags: u32,
    pub texture_handle: Option<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleDrawPath {
    Mesh,
    Quad,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleBlendMode {
    Opaque,
    Alpha,
    Additive,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParticleRenderPlan {
    pub path: ParticleDrawPath,
    pub texture: Option<u32>,
    pub blend: ParticleBlendMode,
    /// ZWRITEENABLE was turned off and must be restored (native zwrite>0).
    pub zwrite_restore: bool,
    /// Quad path: primitives = quads*2, vertices = quads*6.
    pub quad_quads: Option<u32>,
}
/// Original part_renderer::m_RenderFGroup state arms, RVA 0x3d0e0
/// (differential body; every D3D8 call behind the draw backend).
/// I3d_particle.cpp:286-310 consults NEITHER fade NOR the opaque arm: null
/// material takes ALPHASTATE_NONE with a NULL texture and no zwrite kill;
/// any material kills zwrite and blends (ALPHA with PARTF_ALPHA_MASK, else
/// ADD_ALPHA). Returns (blend, zwrite_restore, textured).
pub fn particle_renderer_render_plan(
    part_flags: u32,
    has_material: bool,
    has_texture: bool,
) -> (ParticleBlendMode, bool, bool) {
    if !has_material {
        return (ParticleBlendMode::Opaque, false, false);
    }
    if part_flags & PARTF_ALPHA_MASK != 0 {
        (ParticleBlendMode::Alpha, true, has_texture)
    } else {
        (ParticleBlendMode::Additive, true, has_texture)
    }
}
/// Original part_base::m_RenderFGroup state arms, RVA 0x41320 (contract: no
/// Ghidra body — reference-reported arms with a disclosed device boundary).
/// Raw `fade`/`transparency` cross so each arm applies its own NaN rule
/// (I3d_particle.cpp:1267-1290): the mat==NULL arm tests
/// `fade*transparency < 1.0` (NaN is false → opaque, no zwrite kill); the
/// material arm tests `OPAQUE && >= 1.0` first (NaN fails it) then
/// `& PARTF_ALPHA_MASK`.
pub fn part_base_render_plan(
    part_flags: u32,
    fade: f32,
    transparency: f32,
    material: Option<&ParticleMaterialView>,
    group_len: u32,
) -> ParticleRenderPlan {
    let path = if part_flags & PARTF_MESH != 0 {
        ParticleDrawPath::Mesh
    } else {
        ParticleDrawPath::Quad
    };
    let product = fade * transparency;
    let (blend, zwrite_restore, texture) = match material {
        None => {
            if product < 1.0 {
                (ParticleBlendMode::Alpha, true, None)
            } else {
                (ParticleBlendMode::Opaque, false, None)
            }
        }
        Some(mat) => {
            let opaque = part_flags & PARTF_OPAQUE != 0 && product >= 1.0;
            if opaque {
                (ParticleBlendMode::Opaque, false, mat.texture_handle)
            } else if part_flags & PARTF_ALPHA_MASK != 0 {
                (ParticleBlendMode::Alpha, true, mat.texture_handle)
            } else {
                (ParticleBlendMode::Additive, true, mat.texture_handle)
            }
        }
    };
    ParticleRenderPlan {
        path,
        texture,
        blend,
        zwrite_restore,
        quad_quads: (path == ParticleDrawPath::Quad).then_some(group_len),
    }
}

/// Renderer buffer residency (m_Free order in the 0x3cca0 dtor:
/// mesh_vb, mesh_ib, quad_vb, quad_ib; allocation/lock behind the backend).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ParticleRendererBuffers {
    pub mesh_vb: bool,
    pub mesh_ib: bool,
    pub quad_uv: bool,
    pub quad_idx: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct I3dParticleVisualState {
    pub buffers: ParticleRendererBuffers,
}
pub trait ParticleBufferBackend {
    fn free_particle_buffers(&mut self, order: [&str; 4]);
}

impl ModelArena {
    fn emitor_state_ref(&self, key: FrameKey) -> Result<&I3dEmitorState, ModelError> {
        self.frame(key)?
            .emitor
            .as_ref()
            .ok_or(ModelError::EmitorFrameRequired(key))
    }
    fn emitor_state_mut(&mut self, key: FrameKey) -> Result<&mut I3dEmitorState, ModelError> {
        let index = self.frame_index(key)?;
        self.frames[index]
            .emitor
            .as_mut()
            .ok_or(ModelError::EmitorFrameRequired(key))
    }
    fn emitor_resolve_sector(
        &self,
        mut current: Option<FrameKey>,
    ) -> Result<Option<SectorKey>, ModelError> {
        while let Some(frame) = current {
            if self.state(frame)?.kind == FrameKind::Sector {
                return Ok(Some(SectorKey(frame)));
            }
            current = self.links(frame)?.parent;
        }
        Ok(None)
    }

    /// Original ctor, RVA 0x3de50 (differential: 40 insn/1 BB + base ctor
    /// 0x1001a7f0 / Reset callees). Order: elements cap 0x20, Model carrier,
    /// mesh None, Reset defaults, counters/sector cleared, driver id alloc
    /// (native next_emitor_id++ ushort, backend contract).
    pub fn create_emitor_frame(
        &mut self,
        backend: &mut impl EmitorParamBackend,
    ) -> Result<FrameKey, ModelError> {
        let state = I3dFrameState::model();
        debug_assert!(state.kind == FrameKind::Model);
        let key = self.insert(state, None, Vec::new(), Vec::new(), None)?;
        let index = self.frame_index(key)?;
        let mut emitor = I3dEmitorState::reset_defaults();
        emitor.elements.reserve(0x20);
        emitor.id = backend.alloc_emitor_id();
        self.frames[index].emitor = Some(emitor);
        Ok(key)
    }

    /// Original dtor, RVA 0x3df20 (differential: 45 insn/6 BB). Order-exact:
    /// element backlinks cleared in index order (backend contract — the
    /// native `(*p)->emitor = NULL` walk), mesh Release-if-present
    /// (backend), elements storage dropped, mesh None. Base dtor stays with
    /// its producer.
    pub fn destroy_emitor_derived(
        &mut self,
        key: FrameKey,
        backend: &mut impl EmitorMeshBackend,
        clear_element: &mut impl FnMut(FrameKey),
    ) -> Result<(), ModelError> {
        self.emitor_state_ref(key)?;
        let elements = std::mem::take(&mut self.emitor_state_mut(key)?.elements);
        for element in &elements {
            clear_element(*element);
        }
        let mesh = self.emitor_state_mut(key)?.mesh.take();
        if let Some(mesh) = mesh {
            backend.release_mesh(mesh);
        }
        Ok(())
    }

    /// Original SetMesh, RVA 0x3dfc0 (differential: 19 insn/7 BB). Equal
    /// pointer returns with no effects; old Release precedes new AddRef
    /// (backend) and the store.
    pub fn emitor_set_mesh(
        &mut self,
        key: FrameKey,
        mesh: Option<EmitorMeshHandle>,
        backend: &mut impl EmitorMeshBackend,
    ) -> Result<(), ModelError> {
        self.emitor_state_ref(key)?;
        if self.emitor_state_ref(key)?.mesh == mesh {
            return Ok(());
        }
        if let Some(old) = self.emitor_state_mut(key)?.mesh.take() {
            backend.release_mesh(old);
        }
        if let Some(new) = mesh {
            backend.retain_mesh(new);
        }
        self.emitor_state_mut(key)?.mesh = mesh;
        Ok(())
    }

    /// SetParam wiring (RVA 0x3dff0, contract). Pure arms in
    /// emitor_set_param; PRE_TICK re-enters tick_emitor via the backend;
    /// MAX_PARTICLES runs SetOn(true) after the store, exactly as natively.
    pub fn emitor_set_param(
        &mut self,
        key: FrameKey,
        param: u32,
        value: u32,
        backend: &mut impl EmitorParamBackend,
    ) -> Result<u32, ModelError> {
        self.emitor_state_ref(key)?;
        let index = self.frame_index(key)?;
        let code = emitor_set_param(
            self.frames[index].emitor.as_mut().unwrap(),
            key,
            param,
            value,
            backend,
        );
        if code == EMITOR_OK && ParticleParam::from_u32(param) == Some(ParticleParam::MaxParticles)
        {
            self.emitor_set_on(key, true)?;
        }
        Ok(code)
    }

    /// GetParam wiring (RVA 0x3e660, contract). Colors cross as opaque
    /// words (native pointer returns); the arena supplies stable handles.
    pub fn emitor_get_param(&self, key: FrameKey, param: u32) -> Result<u32, ModelError> {
        let state = self.emitor_state_ref(key)?;
        // Stable per-arena color words (bit tags, never host pointers).
        const COLOR_WORD: u32 = 0xE410_C010;
        const COLOR_END_WORD: u32 = 0xE410_C0BE;
        Ok(emitor_get_param(state, param, COLOR_WORD, COLOR_END_WORD))
    }

    /// Reset wiring (RVA 0x3e930, differential). Native Reset stores
    /// parameters + part_flags only; runtime words (time, emit countdown,
    /// emitted count) are untouched, as are mesh/id/elements/sector.
    pub fn emitor_reset(&mut self, key: FrameKey) -> Result<(), ModelError> {
        self.emitor_state_ref(key)?;
        let index = self.frame_index(key)?;
        let slot = self.frames[index].emitor.as_mut().unwrap();
        let mesh = slot.mesh;
        let sector = slot.sector;
        let id = slot.id;
        let elements = std::mem::take(&mut slot.elements);
        let time_ms = slot.time_ms;
        let emit_countdown = slot.emit_countdown;
        let num_emitted = slot.num_emitted;
        *slot = I3dEmitorState::reset_defaults();
        slot.mesh = mesh;
        slot.sector = sector;
        slot.id = id;
        slot.elements = elements;
        slot.time_ms = time_ms;
        slot.emit_countdown = emit_countdown;
        slot.num_emitted = num_emitted;
        Ok(())
    }

    /// Original SetOn, RVA 0x3eac0 (contract: bool is an 8-bit stack word
    /// per MOV CL,[ESP+8]; RET 8 tail). ON-bit via projected graph flags;
    /// counters reset only on true, in native order.
    pub fn emitor_set_on(&mut self, key: FrameKey, on: bool) -> Result<(), ModelError> {
        self.emitor_state_ref(key)?;
        let flags = self.state(key)?.flags;
        let (flags, reset) = apply_emitor_set_on(flags, on);
        self.graph
            .projected_state_mut(key)
            .map_err(ModelError::from)?
            .flags = flags;
        if reset {
            let state = self.emitor_state_mut(key)?;
            state.num_emitted = 0;
            state.emit_countdown = 0;
            state.time_ms = 0;
        }
        Ok(())
    }

    /// Original UpdateV, RVA 0x3eb10 (contract). Dirty flags drop the
    /// cached sector, nothing else.
    pub fn emitor_update(&mut self, key: FrameKey) -> Result<(), ModelError> {
        self.emitor_state_ref(key)?;
        if emitor_update_v(self.state(key)?.flags) {
            self.emitor_state_mut(key)?.sector = None;
        }
        Ok(())
    }

    /// Original Tick driver, RVA 0x3eb30 (contract: SEH prologue + vtable
    /// slot evidenced; loop interior behind element backend). Live rereads
    /// after every backend call; sector resolved once while null (a missing
    /// sector returns with consumed time retained, as natively); the
    /// max-cutoff stores the decremented time + incremented counter BEFORE
    /// SetOn(false), as natively. RNG samples are drawn in native stream
    /// order (pos ×3, dir ×3, life, dir_period, rot, grow, emit) before the
    /// derived words are computed; see the `EmitorRandSamples` interleave
    /// disclosure. The native `sector->scene == NULL` early return (after a
    /// successful resolve) is NOT modeled: the emission backend owns
    /// scene/tick-list admission, so a null scene surfaces as a backend
    /// failure, never a silent return (disclosed).
    pub fn tick_emitor(
        &mut self,
        key: FrameKey,
        dt_ms: u32,
        backend: &mut impl EmitorEmissionBackend,
    ) -> Result<(), ModelError> {
        self.emitor_state_ref(key)?;
        if self.state(key)?.flags & FRMFLAGS_ON == 0 {
            return Ok(());
        }
        {
            let state = self.emitor_state_mut(key)?;
            state.time_ms = state.time_ms.wrapping_add(dt_ms);
        }
        loop {
            let snapshot = self.emitor_state_ref(key)?.clone();
            match emitor_tick_step(
                true,
                snapshot.time_ms,
                snapshot.emit_countdown,
                snapshot.num_emitted,
                snapshot.max_particles,
            ) {
                EmitorTickStep::Wait => break,
                EmitorTickStep::SwitchedOff { time_left, emitted } => {
                    {
                        let state = self.emitor_state_mut(key)?;
                        state.time_ms = time_left;
                        state.num_emitted = emitted;
                    }
                    self.emitor_set_on(key, false)?;
                    break;
                }
                EmitorTickStep::Emit { time_left } => {
                    {
                        let state = self.emitor_state_mut(key)?;
                        state.time_ms = time_left;
                        state.num_emitted = state.num_emitted.wrapping_add(1);
                    }
                    if self.emitor_state_ref(key)?.sector.is_none() {
                        let parent = self.links(key)?.parent;
                        let sector = self.emitor_resolve_sector(parent)?;
                        match sector {
                            None => break,
                            Some(sector) => {
                                self.emitor_state_mut(key)?.sector = Some(sector);
                            }
                        }
                    }
                    let snapshot = self.emitor_state_ref(key)?.clone();
                    let mut pos_jitter = [0.0f32; 3];
                    for lane in pos_jitter.iter_mut() {
                        *lane = backend.rand_float();
                    }
                    let mut dir_jitter = [0.0f32; 3];
                    for lane in dir_jitter.iter_mut() {
                        *lane = backend.rand_float();
                    }
                    let samples = EmitorRandSamples {
                        pos_jitter,
                        dir_jitter,
                        life: backend.rand_float(),
                        dir_period: backend.rand_float(),
                        rot: backend.rand_float(),
                        grow: backend.rand_float(),
                        emit: backend.rand_float(),
                    };
                    let derived = emitor_derived_params(
                        snapshot.life_time,
                        snapshot.life_time_rnd,
                        snapshot.fade_in,
                        snapshot.fade_out,
                        snapshot.rot_speed_1,
                        snapshot.rot_speed_2,
                        snapshot.grow_1,
                        snapshot.grow_2,
                        snapshot.emit_time_ms,
                        snapshot.emit_time_rnd,
                        snapshot.dir_time,
                        snapshot.dir_time_rnd,
                        &samples,
                    );
                    let Some(derived) = derived else {
                        break;
                    };
                    // Native tail: `emit_time` store first, then the newborn
                    // `e->Tick(late)` with `late = time - emit_base` gated on
                    // `time > emit_time` and `late > 0`. The element Tick
                    // itself stays behind the backend via `late_ms`.
                    let late = emitor_late_ms(time_left, snapshot.emit_time_ms, derived.emit_time);
                    let input = EmitorEmissionInput {
                        sector: snapshot.sector.unwrap(),
                        mesh: snapshot.mesh,
                        billboard: snapshot.billboard,
                        rot_axis: snapshot.rot_axis,
                        axis_mode: snapshot.axis_mode,
                        own_part_flags: snapshot.part_flags,
                        emitor_id: snapshot.id,
                        emitter_scale: [1.0, 1.0, 1.0],
                        emitter_pos: [0.0, 0.0, 0.0],
                        emitter_matrix: SMatrix::identity(),
                        scale: snapshot.scale,
                        pos_rnd: snapshot.pos_rnd,
                        dir_rnd: snapshot.dir_rnd,
                        velocity: snapshot.velocity,
                        velocity_end: snapshot.velocity_end,
                        velocity_world: snapshot.velocity_world,
                        velocity_end_world: snapshot.velocity_end_world,
                        color: snapshot.color,
                        color_end: snapshot.color_end,
                        derived,
                        samples,
                        late_ms: late,
                    };
                    backend.emit_particle(key, &input)?;
                    let state = self.emitor_state_mut(key)?;
                    state.emit_countdown = derived.emit_time;
                }
            }
        }
        Ok(())
    }

    /// InitParticle wiring, RVA 0x3cbb0 (differential). Host replaces the
    /// global (native leaks a live visual here — disclosed hardening).
    pub fn init_particle_visual(&mut self) {
        self.particle_visual = Some(I3dParticleVisualState {
            buffers: ParticleRendererBuffers::default(),
        });
    }

    /// CloseParticle wiring, RVA 0x3cc70 (differential). Null-safe; frees
    /// in dtor order (mesh_vb, mesh_ib, quad_vb, quad_ib) via backend.
    pub fn close_particle_visual(&mut self, backend: &mut impl ParticleBufferBackend) {
        if self.particle_visual.is_some() {
            backend.free_particle_buffers(["mesh_vb", "mesh_ib", "quad_vb", "quad_ib"]);
            self.particle_visual = None;
        }
    }

    pub fn emitor_state(&self, key: FrameKey) -> Result<&I3dEmitorState, ModelError> {
        self.emitor_state_ref(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct LogBackend {
        samples: Vec<f32>,
        ticks: Vec<(FrameKey, u32)>,
        tick_fail: bool,
        colors: std::collections::BTreeMap<u32, [f32; 3]>,
        next_id: u16,
        retained: Vec<EmitorMeshHandle>,
        released: Vec<EmitorMeshHandle>,
        cleared_elements: Vec<FrameKey>,
        freed_orders: Vec<[String; 4]>,
        order: Vec<&'static str>,
        last_late: Option<u32>,
    }
    impl EmitorParamBackend for LogBackend {
        fn rand_float(&mut self) -> f32 {
            self.order.push("rand");
            self.samples.remove(0)
        }
        fn tick_emitor_for(&mut self, key: FrameKey, ms: u32) -> Result<(), ModelError> {
            self.order.push("tick");
            self.ticks.push((key, ms));
            if self.tick_fail {
                return Err(ModelError::AllocationOverflow);
            }
            Ok(())
        }
        fn resolve_param_color(&self, word: u32) -> Option<[f32; 3]> {
            self.colors.get(&word).copied()
        }
        fn alloc_emitor_id(&mut self) -> u16 {
            let id = self.next_id;
            self.next_id = self.next_id.wrapping_add(1);
            id
        }
    }
    impl EmitorMeshBackend for LogBackend {
        fn retain_mesh(&mut self, mesh: EmitorMeshHandle) {
            self.retained.push(mesh);
        }
        fn release_mesh(&mut self, mesh: EmitorMeshHandle) {
            self.released.push(mesh);
        }
    }
    impl ParticleBufferBackend for LogBackend {
        fn free_particle_buffers(&mut self, order: [&str; 4]) {
            self.freed_orders.push(order.map(str::to_string));
        }
    }
    impl EmitorEmissionBackend for LogBackend {
        fn emit_particle(
            &mut self,
            _e: FrameKey,
            i: &EmitorEmissionInput,
        ) -> Result<(), ModelError> {
            self.order.push("emit");
            self.last_late = Some(i.late_ms);
            // Stream-order pin: the 11 draws precede this call in native
            // order (pos ×3, dir ×3, life, dir_period, rot, grow, emit).
            assert_eq!(i.samples.pos_jitter, [0.0; 3]);
            assert_eq!(i.samples.dir_jitter, [0.0; 3]);
            Ok(())
        }
    }
    fn log() -> LogBackend {
        LogBackend {
            samples: vec![0.5; 32],
            ticks: Vec::new(),
            tick_fail: false,
            colors: [(7u32, [0.25, 0.5, 0.75]), (8u32, [-1.0, 2.0, f32::NAN])]
                .into_iter()
                .collect(),
            next_id: 0,
            retained: Vec::new(),
            released: Vec::new(),
            cleared_elements: Vec::new(),
            freed_orders: Vec::new(),
            order: Vec::new(),
            last_late: None,
        }
    }

    #[test]
    fn reset_defaults_match_binary_words() {
        let s = I3dEmitorState::reset_defaults();
        assert_eq!(
            (
                s.emit_time_ms,
                s.emit_time_rnd,
                s.life_time,
                s.life_time_rnd
            ),
            (100, 60, 1800, 600)
        );
        assert_eq!(s.scale.to_bits(), 0x3f80_0000);
        assert_eq!(s.grow_1.to_bits(), 0xbe80_0000);
        assert_eq!(s.grow_2.to_bits(), 0x3f19_999a);
        assert_eq!(s.fade_in.to_bits(), 0x3dcc_cccd);
        assert_eq!(s.fade_out.to_bits(), 0x3dcc_cccd);
        assert_eq!(s.dir_time_stray.to_bits(), 0x3dcc_cccd);
        assert_eq!(s.velocity, [0.0, 2.0, 0.0]);
        assert_eq!(s.velocity_end, [2.0, 0.0, 0.0]);
        assert_eq!(s.rot_speed_1.to_bits(), 0xbfa0_d97c);
        assert_eq!(s.rot_speed_2.to_bits(), 0x3fa0_d97c);
        assert_eq!((s.rot_axis, s.axis_mode), (0, 3));
        assert!(s.billboard && !s.omni_sector && !s.velocity_world);
        assert_eq!(s.part_flags, 0x104);
        assert!(s.mesh.is_none() && s.sector.is_none() && s.elements.is_empty());
    }

    #[test]
    fn set_param_roundtrip_all_params() {
        let mut arena = ModelArena::default();
        let mut backend = log();
        let key = arena.create_emitor_frame(&mut backend).unwrap();
        let words: Vec<(u32, u32)> = vec![
            (0, 150),
            (1, 70),
            (2, 2000),
            (3, 500),
            (26, 0x4000_0000),
            (4, 0x3f00_0000),
            (5, 0x3f80_0000),
            (6, 0x3f80_0000),
            (7, 0x3dcc_cccd),
            (8, 40),
            (9, 10),
            (10, 0x3dcc_cccd),
            (11, 0x3e4c_cccd),
            (12, 0x3e4c_cccd),
            (13, 0x4000_0000),
            (14, 0x4000_0000),
            (15, 0x4000_0000),
            (16, 0x4000_0000),
            (17, 0x4000_0000),
            (18, 0x4000_0000),
            (19, 0x203),
            (20, 64),
            (21, 1),
            (22, 1),
            (23, 0),
            (24, 0xbf80_0000),
            (25, 0x3f80_0000),
            (28, 0),
            (29, 7),
            (30, 8),
            (31, 1),
            (32, 2),
            (33, 1),
        ];
        for (param, value) in &words {
            assert_eq!(
                arena.emitor_set_param(key, *param, *value, &mut backend),
                Ok(EMITOR_OK)
            );
        }
        for (param, value) in &words {
            let got = arena.emitor_get_param(key, *param).unwrap();
            if *param == 29 {
                assert_eq!(got, 0xE410_C010);
            } else if *param == 30 {
                assert_eq!(got, 0xE410_C0BE);
            } else {
                assert_eq!(got, *value, "param {param}");
            }
        }
        assert_eq!(arena.emitor_get_param(key, 21).unwrap(), 1);
        assert_eq!(arena.emitor_get_param(key, 27).unwrap(), 0);
        assert_eq!(arena.emitor_get_param(key, 28).unwrap(), 0);
        assert_ne!(arena.state(key).unwrap().flags & FRMFLAGS_ON, 0);
        let s = arena.emitor_state(key).unwrap();
        assert_eq!((s.time_ms, s.emit_countdown, s.num_emitted), (0, 0, 0));
    }

    #[test]
    fn set_param_guard_word_and_residue() {
        let mut arena = ModelArena::default();
        let mut backend = log();
        let before_key = arena.create_emitor_frame(&mut backend).unwrap();
        let key = arena.create_emitor_frame(&mut backend).unwrap();
        let after_key = arena.create_emitor_frame(&mut backend).unwrap();
        let before = arena.emitor_state(before_key).unwrap().clone();
        let after = arena.emitor_state(after_key).unwrap().clone();
        for bad in [34, 35, 99, u32::MAX] {
            assert_eq!(
                arena.emitor_set_param(key, bad, 0xdead, &mut backend),
                Ok(EMITOR_INVALIDPARAMS)
            );
            assert_eq!(arena.emitor_get_param(key, bad).unwrap(), 0);
        }
        assert_eq!(
            arena.emitor_set_param(key, 29, 0, &mut backend),
            Ok(EMITOR_INVALIDPARAMS)
        );
        assert_eq!(arena.emitor_state(key).unwrap().color, [1.0, 1.0, 1.0]);
        assert_eq!(arena.emitor_state(before_key).unwrap(), &before);
        assert_eq!(arena.emitor_state(after_key).unwrap(), &after);
    }

    #[test]
    fn set_param_emit_time_recompute_order() {
        let mut arena = ModelArena::default();
        let mut backend = log();
        let key = arena.create_emitor_frame(&mut backend).unwrap();
        backend.samples = vec![0.5];
        assert_eq!(
            arena.emitor_set_param(key, 0, 120, &mut backend),
            Ok(EMITOR_OK)
        );
        assert_eq!(arena.emitor_state(key).unwrap().emit_countdown, 0);
        assert_eq!(
            arena.emitor_set_param(key, 1, 60, &mut backend),
            Ok(EMITOR_OK)
        );
        // (0.5*60 + 120) truncated = 150.
        assert_eq!(arena.emitor_state(key).unwrap().emit_countdown, 150);
        assert_eq!(backend.order, vec!["rand"]);
    }

    #[test]
    fn set_param_max_particles_turns_on_and_pre_tick_order() {
        let mut arena = ModelArena::default();
        let mut backend = log();
        let key = arena.create_emitor_frame(&mut backend).unwrap();
        arena.emitor_set_on(key, false).unwrap();
        assert_eq!(
            arena.emitor_set_param(key, 20, 10, &mut backend),
            Ok(EMITOR_OK)
        );
        assert_ne!(arena.state(key).unwrap().flags & FRMFLAGS_ON, 0);
        assert_eq!(
            arena.emitor_set_param(key, 23, 0, &mut backend),
            Ok(EMITOR_OK)
        );
        assert!(backend.ticks.is_empty());
        backend.tick_fail = true;
        assert_eq!(
            arena.emitor_set_param(key, 23, 45, &mut backend),
            Ok(EMITOR_INVALIDPARAMS)
        );
        assert_eq!(arena.emitor_state(key).unwrap().pre_tick, 45);
        assert_eq!(backend.ticks, vec![(key, 45)]);
    }

    #[test]
    fn set_param_color_clamp_nan_preserved() {
        let mut arena = ModelArena::default();
        let mut backend = log();
        let key = arena.create_emitor_frame(&mut backend).unwrap();
        assert_eq!(
            arena.emitor_set_param(key, 29, 7, &mut backend),
            Ok(EMITOR_OK)
        );
        assert_eq!(arena.emitor_state(key).unwrap().color, [0.25, 0.5, 0.75]);
        assert_eq!(
            arena.emitor_set_param(key, 30, 8, &mut backend),
            Ok(EMITOR_OK)
        );
        let end = arena.emitor_state(key).unwrap().color_end;
        assert_eq!(end[0].to_bits(), 0.0f32.to_bits());
        assert_eq!(end[1].to_bits(), 1.0f32.to_bits());
        assert!(end[2].is_nan());
    }

    #[test]
    fn set_on_bit_and_counters() {
        let mut arena = ModelArena::default();
        let mut backend = log();
        let key = arena.create_emitor_frame(&mut backend).unwrap();
        arena.emitor_set_on(key, false).unwrap();
        assert_eq!(arena.state(key).unwrap().flags & FRMFLAGS_ON, 0);
        {
            let index = arena.frame_index(key).unwrap();
            let slot = arena.frames[index].emitor.as_mut().unwrap();
            slot.time_ms = 9;
            slot.emit_countdown = 9;
            slot.num_emitted = 9;
        }
        arena.emitor_set_on(key, false).unwrap();
        assert_eq!(arena.emitor_state(key).unwrap().num_emitted, 9);
        arena.emitor_set_on(key, true).unwrap();
        assert_ne!(arena.state(key).unwrap().flags & FRMFLAGS_ON, 0);
        assert_eq!(
            (
                arena.emitor_state(key).unwrap().time_ms,
                arena.emitor_state(key).unwrap().emit_countdown,
                arena.emitor_state(key).unwrap().num_emitted
            ),
            (0, 0, 0)
        );
    }

    #[test]
    fn update_v_mask_split() {
        assert!(emitor_update_v(0x4000_0000));
        assert!(emitor_update_v(0x2000));
        assert!(emitor_update_v(FRMFLAGS_UPDATE_CHANGED));
        assert!(!emitor_update_v(0));
        assert!(!emitor_update_v(0x1000));
        let mut arena = ModelArena::default();
        let mut backend = log();
        let key = arena.create_emitor_frame(&mut backend).unwrap();
        arena.graph.projected_state_mut(key).unwrap().flags = 0x4000_0000;
        arena.emitor_update(key).unwrap();
        assert!(arena.emitor_state(key).unwrap().sector.is_none());
    }

    #[test]
    fn tick_accounting_cutoff_and_late() {
        assert_eq!(emitor_tick_step(false, 100, 10, 0, 0), EmitorTickStep::Wait);
        assert_eq!(emitor_tick_step(true, 5, 10, 0, 0), EmitorTickStep::Wait);
        assert_eq!(
            emitor_tick_step(true, 50, 50, 9, 10),
            EmitorTickStep::Emit { time_left: 0 }
        );
        assert_eq!(
            emitor_tick_step(true, 50, 50, 10, 10),
            EmitorTickStep::SwitchedOff {
                time_left: 0,
                emitted: 11
            }
        );
        assert_eq!(emitor_late_ms(200, 100, 120), 100);
        assert_eq!(emitor_late_ms(120, 100, 120), 0);
        let mut arena = ModelArena::default();
        let mut backend = log();
        backend.samples = vec![0.0; 64];
        let key = arena.create_emitor_frame(&mut backend).unwrap();
        // Sector parent so the emit path runs past sector resolve.
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sector = arena.scene(scene).unwrap().sectors[0];
        arena.link_frame_base(key, Some(sector.frame()), 0).unwrap();
        arena.emitor_set_param(key, 20, 1, &mut backend).unwrap();
        arena.emitor_set_param(key, 0, 10, &mut backend).unwrap();
        arena.tick_emitor(key, 100, &mut backend).unwrap();
        assert_eq!(arena.state(key).unwrap().flags & FRMFLAGS_ON, 0);
        // Cutoff stores the decremented time + incremented counter (native
        // decrements/increments before the test): the EmitTime store zeroes
        // the countdown, so the first emission consumes nothing (time stays
        // 100) and the cutoff on the next due tick stores time 90, count 2.
        let st = arena.emitor_state(key).unwrap();
        assert_eq!((st.time_ms, st.num_emitted), (90, 2));
        // Full 11-draw RNG stream per emission (pos ×3, dir ×3, life,
        // dir_period, rot, grow, emit), then one emit; the cutoff path draws
        // nothing.
        assert_eq!(backend.order.iter().filter(|o| *o == &"rand").count(), 11);
        assert_eq!(backend.order.last(), Some(&"emit"));
        // Newborn late-tick: time_left 100 > new emit_time 10, late = 100-10.
        assert_eq!(backend.last_late, Some(90));
    }

    #[test]
    fn derived_params_formulas() {
        let s = EmitorRandSamples {
            pos_jitter: [0.5; 3],
            dir_jitter: [0.5; 3],
            life: 0.5,
            dir_period: 0.5,
            rot: 0.5,
            grow: 0.5,
            emit: 0.5,
        };
        let d = emitor_derived_params(
            1800, 600, 0.1, 0.1, -1.2566371, 1.2566371, -0.25, 0.6, 100, 60, 0, 0, &s,
        )
        .unwrap();
        assert_eq!(d.life, 2100);
        assert_eq!(d.life_max, 2400);
        assert_eq!(d.dir_period, 0);
        assert_eq!(d.fade_in, 210);
        assert_eq!(d.fade_out, 2100 - 210);
        assert!((d.rot_speed - 0.0).abs() < 1e-6);
        assert!((d.grow - 0.175 * 0.001).abs() < 1e-9);
        assert_eq!(d.emit_time, 130);
        let bad = EmitorRandSamples {
            life: f32::NAN,
            ..s
        };
        assert!(emitor_derived_params(
            1800, 600, 0.1, 0.1, 0.0, 1.0, 0.0, 1.0, 100, 60, 0, 0, &bad
        )
        .is_none());
    }

    #[test]
    fn sort_plan_guards_and_keys() {
        assert!(particle_fgroup_plan(&[]).groups.is_empty());
        let one = vec![ParticleSortEntry {
            list_index: 4,
            sort_group: 9,
            sort_dist: 3.0,
            part_flags: PARTF_OPAQUE | PARTF_UNKNOWN_100,
            fade: 1.0,
            transparency: 1.0,
            texture_uid: Some(0x1_2345),
            has_sector: true,
        }];
        let plan = particle_fgroup_plan(&one);
        assert_eq!(plan.num_opaque, 1);
        assert_eq!(plan.groups[0].sort_value, 0x2345);
        assert!(plan.groups[0].opaque);
        let faded = vec![ParticleSortEntry {
            fade: 0.5,
            ..one[0]
        }];
        let plan = particle_fgroup_plan(&faded);
        assert_eq!(plan.num_transparent, 1);
        assert_eq!(
            plan.groups[0].sort_value,
            (3u32 & PRIM_SORT_DIST_MASK) | PRIM_SORT_ALPHA
        );
        let pair = vec![
            ParticleSortEntry {
                list_index: 7,
                sort_group: 1,
                sort_dist: 100.0,
                part_flags: 0,
                fade: 1.0,
                transparency: 0.5,
                texture_uid: None,
                has_sector: false,
            },
            ParticleSortEntry {
                list_index: 3,
                sort_group: 1,
                sort_dist: 200.0,
                part_flags: 0,
                fade: 1.0,
                transparency: 0.5,
                texture_uid: None,
                has_sector: false,
            },
        ];
        let plan = particle_fgroup_plan(&pair);
        assert_eq!(plan.groups.len(), 1);
        assert_eq!(plan.groups[0].num, 2);
        assert_eq!(
            plan.groups[0].sort_value,
            (150u32 & PRIM_SORT_DIST_MASK) | PRIM_SORT_ALPHA
        );
        // `first` is the top SORTED position (native `h.first = j`), not the
        // original visual index (3): permuted list_index words prove it.
        assert_eq!(plan.groups[0].first, 1);
        let single = vec![ParticleSortEntry {
            list_index: 9,
            ..pair[0]
        }];
        let plan = particle_fgroup_plan(&single);
        assert_eq!(plan.groups[0].first, 0);
    }

    #[test]
    fn color_pack_order_truncation_and_refuse() {
        let packed = part_color_pack(300.0, [0.25, 0.5, 0.75], 1.0, 1.0).unwrap();
        assert_eq!(packed, (225 | (150 << 8) | (75 << 16) | (255 << 24)));
        assert!(part_color_pack(1.0, [f32::NAN, 0.0, 0.0], 1.0, 1.0).is_none());
        assert!(part_color_pack(1.0, [0.0, 0.0, 0.0], 1.0, 2.0).is_none());
    }

    #[test]
    fn render_plan_arms() {
        // part_base arms (RVA 0x41320, I3d_particle.cpp:1267-1290).
        let mat_alpha = ParticleMaterialView {
            flags: MATF_TEXTURE,
            texture_handle: Some(11),
        };
        let p = part_base_render_plan(PARTF_MESH | PARTF_OPAQUE, 1.0, 1.0, Some(&mat_alpha), 3);
        assert_eq!(
            (p.path, p.blend, p.zwrite_restore, p.texture),
            (
                ParticleDrawPath::Mesh,
                ParticleBlendMode::Opaque,
                false,
                Some(11)
            )
        );
        let p = part_base_render_plan(PARTF_OPAQUE | PARTF_ALPHA, 0.5, 1.0, Some(&mat_alpha), 3);
        assert_eq!(
            (p.path, p.blend, p.zwrite_restore),
            (ParticleDrawPath::Quad, ParticleBlendMode::Alpha, true)
        );
        assert_eq!(p.quad_quads, Some(3));
        let mat_plain = ParticleMaterialView {
            flags: 0,
            texture_handle: None,
        };
        let p = part_base_render_plan(PARTF_OPAQUE, 0.5, 1.0, Some(&mat_plain), 1);
        // OPAQUE sits inside PARTF_ALPHA_MASK: faded opaque blends (Alpha),
        // only flag-less faded material adds (Additive, below).
        assert_eq!(p.blend, ParticleBlendMode::Alpha);
        assert_eq!(p.texture, None);
        let p = part_base_render_plan(0, 0.5, 1.0, Some(&mat_plain), 1);
        assert_eq!(p.blend, ParticleBlendMode::Additive);
        let p = part_base_render_plan(0, 0.5, 1.0, None, 2);
        assert_eq!(
            (p.blend, p.zwrite_restore),
            (ParticleBlendMode::Alpha, true)
        );
        let p = part_base_render_plan(0, 1.0, 1.0, None, 2);
        assert_eq!(
            (p.blend, p.zwrite_restore),
            (ParticleBlendMode::Opaque, false)
        );
        // Per-arm NaN rules (I3d_particle.cpp:1267-1290): the mat==NULL arm
        // tests `< 1.0` (NaN false → opaque, no zwrite kill); the material
        // arm tests `OPAQUE && >= 1.0` first (NaN fails it) then the mask.
        let p = part_base_render_plan(0, f32::NAN, 1.0, None, 2);
        assert_eq!(
            (p.blend, p.zwrite_restore),
            (ParticleBlendMode::Opaque, false)
        );
        let p = part_base_render_plan(PARTF_OPAQUE, f32::NAN, 1.0, Some(&mat_plain), 1);
        assert_eq!(p.blend, ParticleBlendMode::Alpha);
        let p = part_base_render_plan(0, f32::NAN, 1.0, Some(&mat_plain), 1);
        assert_eq!(p.blend, ParticleBlendMode::Additive);
    }

    #[test]
    fn renderer_plan_arms_ignore_fade_and_opaque() {
        // part_renderer arms (RVA 0x3d0e0, I3d_particle.cpp:286-310): null
        // material is always (Opaque/NONE, no zwrite kill, untextured);
        // any material kills zwrite and blends by the alpha mask only.
        assert_eq!(
            particle_renderer_render_plan(0, false, false),
            (ParticleBlendMode::Opaque, false, false)
        );
        assert_eq!(
            particle_renderer_render_plan(PARTF_OPAQUE, false, false),
            (ParticleBlendMode::Opaque, false, false)
        );
        // PARTF_MESH|OPAQUE with a texture: mask bit set (0x201&0x3=1) →
        // Alpha + kill, NOT opaque (the base-only opaque arm is absent here).
        assert_eq!(
            particle_renderer_render_plan(PARTF_MESH | PARTF_OPAQUE, true, true),
            (ParticleBlendMode::Alpha, true, true)
        );
        assert_eq!(
            particle_renderer_render_plan(PARTF_UNKNOWN_100, true, false),
            (ParticleBlendMode::Additive, true, false)
        );
    }

    #[test]
    fn ctor_dtor_and_visual_lifecycle_orders() {
        let mut arena = ModelArena::default();
        let mut backend = log();
        let a = arena.create_emitor_frame(&mut backend).unwrap();
        let b = arena.create_emitor_frame(&mut backend).unwrap();
        assert_eq!(arena.emitor_state(a).unwrap().id, 0);
        assert_eq!(arena.emitor_state(b).unwrap().id, 1);
        assert_eq!(arena.state(a).unwrap().kind, FrameKind::Model);
        arena
            .emitor_set_mesh(a, Some(EmitorMeshHandle(5)), &mut backend)
            .unwrap();
        arena
            .emitor_set_mesh(a, Some(EmitorMeshHandle(5)), &mut backend)
            .unwrap();
        assert_eq!(backend.retained, vec![EmitorMeshHandle(5)]);
        arena
            .emitor_set_mesh(a, Some(EmitorMeshHandle(6)), &mut backend)
            .unwrap();
        assert_eq!(backend.released, vec![EmitorMeshHandle(5)]);
        assert_eq!(
            backend.retained,
            vec![EmitorMeshHandle(5), EmitorMeshHandle(6)]
        );
        let (e0, e1) = (FrameKey(501), FrameKey(502));
        {
            let index = arena.frame_index(a).unwrap();
            arena.frames[index].emitor.as_mut().unwrap().elements = vec![e0, e1];
        }
        let mut cleared: Vec<FrameKey> = Vec::new();
        {
            let mut clear = |k: FrameKey| cleared.push(k);
            arena
                .destroy_emitor_derived(a, &mut backend, &mut clear)
                .unwrap();
        }
        backend.cleared_elements = cleared;
        assert_eq!(backend.cleared_elements, vec![e0, e1]);
        assert_eq!(backend.released.last(), Some(&EmitorMeshHandle(6)));
        assert!(arena.emitor_state(a).unwrap().mesh.is_none());
        arena.init_particle_visual();
        assert!(arena.particle_visual.is_some());
        arena.close_particle_visual(&mut backend);
        assert!(arena.particle_visual.is_none());
        assert_eq!(
            backend.freed_orders,
            vec![[
                "mesh_vb".to_string(),
                "mesh_ib".to_string(),
                "quad_vb".to_string(),
                "quad_ib".to_string()
            ]]
        );
        arena.close_particle_visual(&mut backend);
        assert_eq!(backend.freed_orders.len(), 1);
    }
}
