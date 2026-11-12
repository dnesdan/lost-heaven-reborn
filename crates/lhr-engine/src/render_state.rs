#![forbid(unsafe_code)]
//! Cached render-state setters (`render.h` helpers).
//!
//! LS3DF.dll SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! RVAs 0xab40 (`SetBlendState`), 0xae50 (`SetVertexFormat`), 0x29f30
//! (`SetColorOp1`, working name; mafia-re guess, NOT original), 0x29ef0
//! (`SetCullState`, working name; mafia-re guess, NOT original), 0x333d0
//! (`SetClipping`, working name; mafia-re guess, NOT original), 0x659f0
//! (`SetZBias`, working name; mafia-re guess, NOT original), 0x4e390
//! (`SetFog`, working name; mafia-re guess, NOT original), 0xaa30
//! (`SetAlphaState`, working name; mafia-re guess, NOT original), 0xac30
//! (`SetColorOp`, working name; mafia-re guess, NOT original), 0xac60
//! (`SetAlphaOp`, working name; mafia-re guess, NOT original). Pinned mafia-re
//! ab67b13 `ls3df/I3D/render/render.h:13-209` and `ls3df/I3D/I3D.h:4464-4514`.
//!
//! Contract tier this wave: reviewed upstream source plus Rust tests, no
//! Ghidra body claimed. Every setter follows the same shape: cache guard,
//! store-before-switch, then device effects behind a backend trait (the
//! `buffer_managers::BufferBackend` pattern). Unknown selector values store
//! the cache with no device call, mirroring the missing `default` arms.
//! Fog epsilon uses the integer bit comparison `bits < 0x322bcc77` on the
//! absolute difference, not a float `< 1e-8`. Float bits for fog states use
//! plain `to_bits`, matching `I3DFloatAsInt`.

/// Blend selectors (ls3df.h:370-374).
pub const BLENDSTATE_ALPHA: u32 = 0;
/// Blend selectors (ls3df.h:370-374).
pub const BLENDSTATE_ADD_ALPHA: u32 = 1;
/// Blend selectors (ls3df.h:370-374).
pub const BLENDSTATE_ADD: u32 = 2;
/// Blend selectors (ls3df.h:370-374).
pub const BLENDSTATE_MODULATE: u32 = 3;
/// Blend selectors (ls3df.h:370-374).
pub const BLENDSTATE_MODULATE2X: u32 = 4;

/// Alpha selectors (original names, ls3df.h:363-366).
pub const ALPHASTATE_NONE: u32 = 0;
/// Alpha selectors (original names, ls3df.h:363-366).
pub const ALPHASTATE_BLEND: u32 = 1;
/// Alpha selectors (original names, ls3df.h:363-366).
pub const ALPHASTATE_BLEND_ALL: u32 = 2;
/// Alpha selectors (original names, ls3df.h:363-366).
pub const ALPHASTATE_TEST: u32 = 3;

/// `D3DBLEND` factors used by the blend mapping.
pub const D3DBLEND_ZERO: u32 = 1;
/// `D3DBLEND` factors used by the blend mapping.
pub const D3DBLEND_ONE: u32 = 2;
/// `D3DBLEND` factors used by the blend mapping.
pub const D3DBLEND_SRCCOLOR: u32 = 3;
/// `D3DBLEND` factors used by the blend mapping.
pub const D3DBLEND_SRCALPHA: u32 = 5;
/// `D3DBLEND` factors used by the blend mapping.
pub const D3DBLEND_INVSRCALPHA: u32 = 6;
/// `D3DBLEND` factors used by the blend mapping.
pub const D3DBLEND_DESTCOLOR: u32 = 9;

/// Vertex-format selectors (ls3df.h:379-383; 2 and 3 have no names).
pub const VF_LVERTEX: u32 = 0;
/// Vertex-format selectors (ls3df.h:379-383).
pub const VF_LVERTEX2: u32 = 1;
/// Vertex-format selectors (ls3df.h:381-382).
pub const VF_TLVERTEX: u32 = 4;
/// Vertex-format selectors (ls3df.h:381-382).
pub const VF_TLVERTEX2: u32 = 5;
/// Vertex-format selectors (ls3df.h:379-383).
pub const VF_VERTEX: u32 = 6;

/// `D3DCULL` values.
pub const D3DCULL_NONE: u32 = 1;
/// `D3DCULL` values.
pub const D3DCULL_CW: u32 = 2;
/// `D3DCULL` values; aliases `m_CullMode` in `SetCullState`.
pub const D3DCULL_CCW: u32 = 3;

/// Fog-type selectors (render.h:149-151; working names, NOT original).
/// D3D8 fog-mode words programmed by SetFog (NONE=0, EXP=1, EXP2=2,
/// LINEAR=3): the FOGTYPE_* selector words (0,1,2) must be mapped, NOT
/// forwarded raw — forwarding LINEAR (0) would program D3DFOG_NONE (off).
pub const D3DFOG_NONE: u32 = 0;
pub const D3DFOG_EXP: u32 = 1;
pub const D3DFOG_EXP2: u32 = 2;
pub const D3DFOG_LINEAR: u32 = 3;
pub const FOGTYPE_LINEAR: u32 = 0;
/// Fog-type selectors (render.h:149-151; working names, NOT original).
pub const FOGTYPE_EXP: u32 = 1;
/// Fog-type selectors (render.h:149-151; working names, NOT original).
pub const FOGTYPE_EXP2: u32 = 2;

/// Hardware capability selecting table fog (ls3df.h:353).
pub const HWCAPS_TABLEFOG: u32 = 0x4;
/// Marginal-zero bit mask (ls3df.h:49).
pub const MRG_ZERO_BITS: u32 = 0x322bcc77;

/// Typed cache for all seven setters plus the driver profile they read.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderState {
    /// `m_CurrBlendState`.
    pub curr_blend: u32,
    /// `m_CurrAlphaState` (working-name cache, NOT an original symbol).
    pub curr_alpha_state: u32,
    /// `m_CurrColorOp` (working-name cache, NOT an original symbol).
    pub curr_color_op: u32,
    /// `m_CurrAlphaOp` (working-name cache, NOT an original symbol).
    pub curr_alpha_op: u32,
    /// `m_AlphaRef` live value read by the TEST arm.
    pub alpha_ref: u32,
    /// `m_CurrVertexFormat`.
    pub curr_vertex_format: u32,
    /// `m_CurrColorOp1` (working-name cache, NOT an original symbol).
    pub curr_color_op1: u32,
    /// `m_CurrCullState` (working-name cache, NOT an original symbol).
    pub curr_cull: u32,
    /// Driver `m_CullMode` used when `cull == D3DCULL_CCW`.
    pub cull_mode: u32,
    /// `m_CurrClipState` (working-name cache, NOT an original symbol).
    pub curr_clip: u32,
    /// `m_CurrZBias` bits (working-name cache, NOT an original symbol).
    pub curr_zbias: u32,
    /// `m_CurrFogState` (working-name cache, NOT an original symbol).
    pub curr_fog_state: u32,
    /// `m_CurrFogType` (working-name cache, NOT an original symbol).
    pub curr_fog_type: u32,
    /// `m_CurrFogStart`.
    pub curr_fog_start: f32,
    /// `m_CurrFogEnd`.
    pub curr_fog_end: f32,
    /// `m_CurrFogColor`.
    pub curr_fog_color: u32,
    /// `m_HardwareCaps`.
    pub hardware_caps: u32,
}

impl Default for RenderState {
    fn default() -> Self {
        Self {
            curr_blend: 0,
            curr_alpha_state: 0,
            curr_color_op: 0,
            curr_alpha_op: 0,
            // LS3DF render-state default, borrowed from the IGraph global:
            // `uint m_AlphaRef = 0x20` (IGraph.cpp:403; reset at :534).
            alpha_ref: 0x20,
            curr_vertex_format: 0,
            curr_color_op1: 0,
            curr_cull: 0,
            cull_mode: D3DCULL_CCW,
            curr_clip: 0,
            curr_zbias: 0,
            curr_fog_state: 0,
            curr_fog_type: 0,
            curr_fog_start: 0.0,
            curr_fog_end: 0.0,
            curr_fog_color: 0,
            hardware_caps: 0,
        }
    }
}

/// Device effects for `SetBlendState` behind a backend trait.
pub trait BlendBackend {
    /// `SetRenderState(D3DRS_SRCBLEND, mode)`.
    fn set_src_blend(&mut self, mode: u32);
    /// `SetRenderState(D3DRS_DESTBLEND, mode)`.
    fn set_dest_blend(&mut self, mode: u32);
}

/// Device effects for `SetAlphaState` behind a backend trait.
pub trait AlphaStateBackend {
    /// `SetRenderState(D3DRS_ALPHABLENDENABLE, enable)`.
    fn set_alpha_blend_enable(&mut self, enable: bool);
    /// `SetRenderState(D3DRS_ALPHATESTENABLE, enable)`.
    fn set_alpha_test_enable(&mut self, enable: bool);
    /// `SetRenderState(D3DRS_ALPHAREF, value)`.
    fn set_alpha_ref(&mut self, value: u32);
}

/// Device effects for `SetColorOp` behind a backend trait.
pub trait ColorOpBackend {
    /// `SetTextureStageState(0, D3DTSS_COLOROP, op)`.
    fn set_color_op(&mut self, op: u32);
}

/// Device effects for `SetAlphaOp` behind a backend trait.
pub trait AlphaOpBackend {
    /// `SetTextureStageState(0, D3DTSS_ALPHAOP, op)`.
    fn set_alpha_op(&mut self, op: u32);
}

/// Typed vertex-shader selection for `SetVertexFormat`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VertexShader {
    LVertex,
    LVertex2,
    Diffuse,
    DiffuseTex1,
    TlVertex,
    TlVertex2,
    Vertex,
}

/// Device effects for `SetVertexFormat` behind a backend trait.
pub trait VertexFormatBackend {
    /// `SetVertexShader(...)` selection.
    fn set_vertex_shader(&mut self, shader: VertexShader);
}

/// Device effects for `SetColorOp1` behind a backend trait.
pub trait ColorOp1Backend {
    /// `SetTextureStageState(1, D3DTSS_COLOROP, op)`.
    fn set_color_op1(&mut self, op: u32);
}

/// Device effects for `SetCullState` behind a backend trait.
pub trait CullBackend {
    /// `SetRenderState(D3DRS_CULLMODE, mode)`.
    fn set_cull_mode(&mut self, mode: u32);
}

/// Device effects for `SetClipping` behind a backend trait.
pub trait ClippingBackend {
    /// `SetRenderState(D3DRS_CLIPPING, TRUE/FALSE)`.
    fn set_clipping(&mut self, enable: bool);
}

/// Device effects for `SetZBias` behind a backend trait.
pub trait ZBiasBackend {
    /// `SetRenderState(D3DRS_ZBIAS, bias)`.
    fn set_zbias(&mut self, bias: i32);
}

/// Device effects for `SetFog` behind a backend trait.
pub trait FogBackend {
    /// `SetRenderState(D3DRS_FOGENABLE, ...)`.
    fn set_fog_enable(&mut self, enable: bool);
    /// Fog mode select; `table` mirrors `HWCAPS_TABLEFOG`.
    fn set_fog_mode(&mut self, table: bool, mode: u32);
    /// `SetRenderState(D3DRS_FOGEND, bits)`.
    fn set_fog_end(&mut self, bits: u32);
    /// `SetRenderState(D3DRS_FOGSTART, bits)`.
    fn set_fog_start(&mut self, bits: u32);
    /// `SetRenderState(D3DRS_FOGDENSITY, bits)`.
    fn set_fog_density(&mut self, bits: u32);
    /// `SetRenderState(D3DRS_FOGCOLOR, color)`.
    fn set_fog_color(&mut self, color: u32);
}

/// `IsAbsMrgZero` (ls3df.h:57-70) as integer bits: `|v| < 1e-8` via
/// `abs().to_bits() < MRG_ZERO_BITS`. NaN is never marginal zero.
fn is_abs_mrg_zero(value: f32) -> bool {
    value.abs().to_bits() < MRG_ZERO_BITS
}

/// LS3DF RVA 0xab40 `SetBlendState` (mafia-re
/// `ls3df/I3D/render/render.h:13-44`). Cache guard, store-before-switch,
/// then the SRC/DEST pair. Unknown selectors store with no device call
/// (the `default: assert(0)` arm).
pub fn set_blend_state(state: &mut RenderState, value: u32, backend: &mut impl BlendBackend) {
    if state.curr_blend == value {
        return;
    }
    state.curr_blend = value;
    match value {
        BLENDSTATE_ALPHA => {
            backend.set_src_blend(D3DBLEND_SRCALPHA);
            backend.set_dest_blend(D3DBLEND_INVSRCALPHA);
        }
        BLENDSTATE_ADD_ALPHA => {
            backend.set_src_blend(D3DBLEND_SRCALPHA);
            backend.set_dest_blend(D3DBLEND_ONE);
        }
        BLENDSTATE_ADD => {
            backend.set_src_blend(D3DBLEND_ONE);
            backend.set_dest_blend(D3DBLEND_ONE);
        }
        BLENDSTATE_MODULATE => {
            backend.set_src_blend(D3DBLEND_DESTCOLOR);
            backend.set_dest_blend(D3DBLEND_ZERO);
        }
        BLENDSTATE_MODULATE2X => {
            backend.set_src_blend(D3DBLEND_DESTCOLOR);
            backend.set_dest_blend(D3DBLEND_SRCCOLOR);
        }
        _ => {}
    }
}

/// LS3DF RVA 0xaa30 `SetAlphaState` (mafia-re `ls3df/I3D/I3D.h:4464`;
/// working name, mafia-re guess, NOT an original symbol; LS3DF11 RVA
/// 0xa6c0). Cache guard, store-before-switch, then the 4-arm blend/test/ref
/// switch. Unknown selectors store with no device call (the missing
/// `default` arm). The TEST arm rereads the live `alpha_ref` at effect
/// time, matching the native `m_AlphaRef` read. No floating-point work.
pub fn set_alpha_state(state: &mut RenderState, value: u32, backend: &mut impl AlphaStateBackend) {
    if state.curr_alpha_state == value {
        return;
    }
    state.curr_alpha_state = value;
    match value {
        ALPHASTATE_NONE => {
            backend.set_alpha_blend_enable(false);
            backend.set_alpha_test_enable(false);
        }
        ALPHASTATE_BLEND => {
            backend.set_alpha_blend_enable(true);
            backend.set_alpha_test_enable(true);
            backend.set_alpha_ref(1);
        }
        ALPHASTATE_TEST => {
            backend.set_alpha_blend_enable(false);
            backend.set_alpha_test_enable(true);
            backend.set_alpha_ref(state.alpha_ref);
        }
        ALPHASTATE_BLEND_ALL => {
            backend.set_alpha_blend_enable(true);
            backend.set_alpha_test_enable(true);
            backend.set_alpha_ref(0);
        }
        _ => {}
    }
}

/// LS3DF RVA 0xac30 `SetColorOp` (mafia-re `ls3df/I3D/I3D.h:4495`;
/// working name, mafia-re guess, NOT an original symbol; LS3DF11 RVA
/// 0xa8c0). Cache guard, store, then the stage-0 color op. Distinct from
/// stage-1 `set_color_op1` at 0x29f30. No floating-point work.
pub fn set_color_op(state: &mut RenderState, op: u32, backend: &mut impl ColorOpBackend) {
    if state.curr_color_op == op {
        return;
    }
    state.curr_color_op = op;
    backend.set_color_op(op);
}

/// LS3DF RVA 0xac60 `SetAlphaOp` (mafia-re `ls3df/I3D/I3D.h:4506`;
/// working name, mafia-re guess, NOT an original symbol; LS3DF11 RVA
/// 0xa8f0). Cache guard, store, then the stage-0 alpha op. No
/// floating-point work.
pub fn set_alpha_op(state: &mut RenderState, op: u32, backend: &mut impl AlphaOpBackend) {
    if state.curr_alpha_op == op {
        return;
    }
    state.curr_alpha_op = op;
    backend.set_alpha_op(op);
}

/// LS3DF RVA 0xae50 `SetVertexFormat` (mafia-re
/// `ls3df/I3D/render/render.h:46-77`). Cache guard, store-before-switch,
/// then the shader select. Unknown formats store with no device call.
pub fn set_vertex_format(
    state: &mut RenderState,
    format: u32,
    backend: &mut impl VertexFormatBackend,
) {
    if state.curr_vertex_format == format {
        return;
    }
    state.curr_vertex_format = format;
    match format {
        VF_LVERTEX => backend.set_vertex_shader(VertexShader::LVertex),
        VF_LVERTEX2 => backend.set_vertex_shader(VertexShader::LVertex2),
        2 => backend.set_vertex_shader(VertexShader::Diffuse),
        3 => backend.set_vertex_shader(VertexShader::DiffuseTex1),
        VF_TLVERTEX => backend.set_vertex_shader(VertexShader::TlVertex),
        VF_TLVERTEX2 => backend.set_vertex_shader(VertexShader::TlVertex2),
        VF_VERTEX => backend.set_vertex_shader(VertexShader::Vertex),
        _ => {}
    }
}

/// LS3DF RVA 0x29f30 `SetColorOp1` (mafia-re
/// `ls3df/I3D/render/render.h:79-90`; working name, mafia-re guess, NOT an
/// original symbol). Cache guard, store, then stage-1 color op.
pub fn set_color_op1(state: &mut RenderState, op: u32, backend: &mut impl ColorOp1Backend) {
    if state.curr_color_op1 == op {
        return;
    }
    state.curr_color_op1 = op;
    backend.set_color_op1(op);
}

/// LS3DF RVA 0x29ef0 `SetCullState` (mafia-re
/// `ls3df/I3D/render/render.h:92-106`; working name, mafia-re guess, NOT an
/// original symbol). Cache guard, store, then `m_CullMode` aliasing for
/// `D3DCULL_CCW`.
pub fn set_cull_state(state: &mut RenderState, cull: u32, backend: &mut impl CullBackend) {
    if state.curr_cull == cull {
        return;
    }
    state.curr_cull = cull;
    if cull == D3DCULL_CCW {
        backend.set_cull_mode(state.cull_mode);
    } else {
        backend.set_cull_mode(cull);
    }
}

/// LS3DF RVA 0x333d0 `SetClipping` (mafia-re
/// `ls3df/I3D/render/render.h:108-122`; working name, mafia-re guess, NOT an
/// original symbol). Cache guard, store, then `TRUE` only for exactly 1.
pub fn set_clipping(state: &mut RenderState, clip: u32, backend: &mut impl ClippingBackend) {
    if state.curr_clip == clip {
        return;
    }
    state.curr_clip = clip;
    backend.set_clipping(clip == 1);
}

/// LS3DF RVA 0x659f0 `SetZBias` (mafia-re
/// `ls3df/I3D/render/render.h:127-140`; working name, mafia-re guess, NOT an
/// original symbol). The cache is `uint`; the comparison casts the `int`
/// bias, preserving negative bit patterns.
pub fn set_zbias(state: &mut RenderState, bias: i32, backend: &mut impl ZBiasBackend) {
    if state.curr_zbias == bias as u32 {
        return;
    }
    state.curr_zbias = bias as u32;
    backend.set_zbias(bias);
}

/// LS3DF RVA 0x4e390 `SetFog` (mafia-re `ls3df/I3D/render/render.h:153-209`;
/// working name, mafia-re guess, NOT an original symbol). Fog-enable flip
/// stores first and returns after disabling. The mode and start/end/density
/// updates use integer marginal-zero compares; the second switch reads the
/// cached type. Color updates last.
#[allow(clippy::too_many_arguments)]
pub fn set_fog(
    state: &mut RenderState,
    fog_state: u32,
    fog_type: u32,
    start: f32,
    end: f32,
    color: u32,
    backend: &mut impl FogBackend,
) {
    if state.curr_fog_state != fog_state {
        state.curr_fog_state = fog_state;
        if fog_state == 1 {
            backend.set_fog_enable(true);
        } else {
            backend.set_fog_enable(false);
            return;
        }
    } else if fog_state == 0 {
        return;
    }
    if state.curr_fog_type != fog_type {
        state.curr_fog_type = fog_type;
        let table = (state.hardware_caps & HWCAPS_TABLEFOG) != 0;
        match fog_type {
            FOGTYPE_LINEAR => backend.set_fog_mode(table, D3DFOG_LINEAR),
            FOGTYPE_EXP => backend.set_fog_mode(table, D3DFOG_EXP),
            FOGTYPE_EXP2 => backend.set_fog_mode(table, D3DFOG_EXP2),
            _ => {}
        }
    }
    match state.curr_fog_type {
        FOGTYPE_LINEAR => {
            if !is_abs_mrg_zero(state.curr_fog_end - end) {
                state.curr_fog_end = end;
                backend.set_fog_end(end.to_bits());
            }
            if !is_abs_mrg_zero(state.curr_fog_start - start) {
                state.curr_fog_start = start;
                backend.set_fog_start(start.to_bits());
            }
        }
        FOGTYPE_EXP | FOGTYPE_EXP2 if !is_abs_mrg_zero(state.curr_fog_start - start) => {
            state.curr_fog_start = start;
            backend.set_fog_density(start.to_bits());
        }
        _ => {}
    }
    if state.curr_fog_color != color {
        state.curr_fog_color = color;
        backend.set_fog_color(color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct BlendLog {
        pairs: Vec<(u32, u32)>,
    }
    impl BlendBackend for BlendLog {
        fn set_src_blend(&mut self, mode: u32) {
            if let Some(last) = self.pairs.last_mut() {
                last.0 = mode;
            } else {
                self.pairs.push((mode, 0));
            }
        }
        fn set_dest_blend(&mut self, mode: u32) {
            if let Some(last) = self.pairs.last_mut() {
                if last.1 == 0 && last.0 != 0 {
                    last.1 = mode;
                } else {
                    self.pairs.push((0, mode));
                }
            } else {
                self.pairs.push((0, mode));
            }
        }
    }

    #[test]
    fn blend_caches_and_maps_all_modes_with_store_before_switch() {
        let mut state = RenderState {
            curr_blend: 99,
            ..Default::default()
        };
        let mut backend = BlendLog { pairs: Vec::new() };
        // Cache hit issues no device call.
        state.curr_blend = BLENDSTATE_ALPHA;
        set_blend_state(&mut state, BLENDSTATE_ALPHA, &mut backend);
        assert!(backend.pairs.is_empty());
        // Each mode maps to its documented pair.
        for (mode, src, dst) in [
            (BLENDSTATE_ALPHA, D3DBLEND_SRCALPHA, D3DBLEND_INVSRCALPHA),
            (BLENDSTATE_ADD_ALPHA, D3DBLEND_SRCALPHA, D3DBLEND_ONE),
            (BLENDSTATE_ADD, D3DBLEND_ONE, D3DBLEND_ONE),
            (BLENDSTATE_MODULATE, D3DBLEND_DESTCOLOR, D3DBLEND_ZERO),
            (BLENDSTATE_MODULATE2X, D3DBLEND_DESTCOLOR, D3DBLEND_SRCCOLOR),
        ] {
            state.curr_blend = 99;
            backend.pairs.clear();
            set_blend_state(&mut state, mode, &mut backend);
            assert_eq!(state.curr_blend, mode);
            assert_eq!(backend.pairs, vec![(src, dst)], "mode {mode}");
        }
        // Unknown stores with no device call.
        state.curr_blend = 0;
        backend.pairs.clear();
        set_blend_state(&mut state, 91, &mut backend);
        assert_eq!(state.curr_blend, 91);
        assert!(backend.pairs.is_empty());
    }

    struct ShaderLog {
        shaders: Vec<VertexShader>,
    }
    impl VertexFormatBackend for ShaderLog {
        fn set_vertex_shader(&mut self, shader: VertexShader) {
            self.shaders.push(shader);
        }
    }

    #[test]
    fn vertex_format_caches_and_selects_each_shader() {
        let mut state = RenderState::default();
        let mut backend = ShaderLog {
            shaders: Vec::new(),
        };
        state.curr_vertex_format = VF_LVERTEX;
        set_vertex_format(&mut state, VF_LVERTEX, &mut backend);
        assert!(backend.shaders.is_empty());
        for (format, shader) in [
            (VF_LVERTEX, VertexShader::LVertex),
            (VF_LVERTEX2, VertexShader::LVertex2),
            (2, VertexShader::Diffuse),
            (3, VertexShader::DiffuseTex1),
            (VF_TLVERTEX, VertexShader::TlVertex),
            (VF_TLVERTEX2, VertexShader::TlVertex2),
            (VF_VERTEX, VertexShader::Vertex),
        ] {
            state.curr_vertex_format = 99;
            backend.shaders.clear();
            set_vertex_format(&mut state, format, &mut backend);
            assert_eq!(state.curr_vertex_format, format);
            assert_eq!(backend.shaders, vec![shader]);
        }
        state.curr_vertex_format = 0;
        backend.shaders.clear();
        set_vertex_format(&mut state, 77, &mut backend);
        assert_eq!(state.curr_vertex_format, 77);
        assert!(backend.shaders.is_empty());
    }

    struct OpLog {
        ops: Vec<u32>,
    }
    impl ColorOp1Backend for OpLog {
        fn set_color_op1(&mut self, op: u32) {
            self.ops.push(op);
        }
    }

    #[test]
    fn color_op1_caches_then_forwards_every_op() {
        let mut state = RenderState {
            curr_color_op1: 4,
            ..Default::default()
        };
        let mut backend = OpLog { ops: Vec::new() };
        set_color_op1(&mut state, 4, &mut backend);
        assert!(backend.ops.is_empty());
        set_color_op1(&mut state, 7, &mut backend);
        assert_eq!(state.curr_color_op1, 7);
        assert_eq!(backend.ops, vec![7]);
    }

    struct CullLog {
        modes: Vec<u32>,
    }
    impl CullBackend for CullLog {
        fn set_cull_mode(&mut self, mode: u32) {
            self.modes.push(mode);
        }
    }

    #[test]
    fn cull_aliases_ccw_through_driver_mode() {
        let mut state = RenderState {
            cull_mode: D3DCULL_CW,
            curr_cull: D3DCULL_NONE,
            ..Default::default()
        };
        let mut backend = CullLog { modes: Vec::new() };
        set_cull_state(&mut state, D3DCULL_NONE, &mut backend);
        assert!(backend.modes.is_empty());
        set_cull_state(&mut state, D3DCULL_CCW, &mut backend);
        assert_eq!(state.curr_cull, D3DCULL_CCW);
        assert_eq!(backend.modes, vec![D3DCULL_CW]);
        backend.modes.clear();
        set_cull_state(&mut state, D3DCULL_NONE, &mut backend);
        assert_eq!(backend.modes, vec![D3DCULL_NONE]);
    }

    struct ClipLog {
        enables: Vec<bool>,
    }
    impl ClippingBackend for ClipLog {
        fn set_clipping(&mut self, enable: bool) {
            self.enables.push(enable);
        }
    }

    #[test]
    fn clipping_caches_and_maps_only_one_to_true() {
        let mut state = RenderState {
            curr_clip: 1,
            ..Default::default()
        };
        let mut backend = ClipLog {
            enables: Vec::new(),
        };
        set_clipping(&mut state, 1, &mut backend);
        assert!(backend.enables.is_empty());
        set_clipping(&mut state, 0, &mut backend);
        assert_eq!(backend.enables, vec![false]);
        backend.enables.clear();
        state.curr_clip = 0;
        set_clipping(&mut state, 2, &mut backend);
        assert_eq!(state.curr_clip, 2);
        assert_eq!(backend.enables, vec![false]);
    }

    struct ZLog {
        biases: Vec<i32>,
    }
    impl ZBiasBackend for ZLog {
        fn set_zbias(&mut self, bias: i32) {
            self.biases.push(bias);
        }
    }

    #[test]
    fn zbias_compares_as_unsigned_bits() {
        let mut state = RenderState::default();
        let mut backend = ZLog { biases: Vec::new() };
        set_zbias(&mut state, 2, &mut backend);
        assert_eq!(state.curr_zbias, 2);
        assert_eq!(backend.biases, vec![2]);
        backend.biases.clear();
        set_zbias(&mut state, 2, &mut backend);
        assert!(backend.biases.is_empty());
        // -1 stores 0xFFFF_FFFF and forwards the signed bias.
        set_zbias(&mut state, -1, &mut backend);
        assert_eq!(state.curr_zbias, 0xffff_ffff);
        assert_eq!(backend.biases, vec![-1]);
    }

    struct FogLog {
        events: Vec<String>,
    }
    impl FogBackend for FogLog {
        fn set_fog_enable(&mut self, enable: bool) {
            self.events.push(format!("enable:{enable}"));
        }
        fn set_fog_mode(&mut self, table: bool, mode: u32) {
            self.events.push(format!("mode:{table}:{mode}"));
        }
        fn set_fog_end(&mut self, bits: u32) {
            self.events.push(format!("end:{bits:#x}"));
        }
        fn set_fog_start(&mut self, bits: u32) {
            self.events.push(format!("start:{bits:#x}"));
        }
        fn set_fog_density(&mut self, bits: u32) {
            self.events.push(format!("density:{bits:#x}"));
        }
        fn set_fog_color(&mut self, color: u32) {
            self.events.push(format!("color:{color:#x}"));
        }
    }

    #[test]
    fn fog_enable_flip_disables_and_returns_without_further_effects() {
        let mut state = RenderState {
            curr_fog_state: 1,
            curr_fog_type: FOGTYPE_LINEAR,
            curr_fog_start: 1.0,
            curr_fog_end: 9.0,
            curr_fog_color: 0x1234,
            ..Default::default()
        };
        let mut backend = FogLog { events: Vec::new() };
        set_fog(
            &mut state,
            0,
            FOGTYPE_LINEAR,
            5.0,
            6.0,
            0x5678,
            &mut backend,
        );
        assert_eq!(state.curr_fog_state, 0);
        assert_eq!(backend.events, vec!["enable:false"]);
        assert_eq!(state.curr_fog_start, 1.0);
        assert_eq!(state.curr_fog_color, 0x1234);
        // Disabled steady state is quiet.
        backend.events.clear();
        set_fog(
            &mut state,
            0,
            FOGTYPE_LINEAR,
            5.0,
            6.0,
            0x5678,
            &mut backend,
        );
        assert!(backend.events.is_empty());
    }

    #[test]
    fn fog_linear_updates_end_start_color_with_marginal_zero_gate() {
        let mut state = RenderState {
            curr_fog_state: 1,
            curr_fog_type: FOGTYPE_LINEAR,
            curr_fog_start: 0.0,
            curr_fog_end: 9.0,
            curr_fog_color: 0,
            ..Default::default()
        };
        let mut backend = FogLog { events: Vec::new() };
        // Marginal difference (5e-9) is below the bit gate: no start/end call.
        let tiny: f32 = 5e-9;
        assert!(tiny.abs().to_bits() < MRG_ZERO_BITS);
        set_fog(
            &mut state,
            1,
            FOGTYPE_LINEAR,
            tiny,
            9.0,
            0xff00,
            &mut backend,
        );
        assert_eq!(state.curr_fog_start, 0.0);
        assert_eq!(backend.events, vec!["color:0xff00"]);
        // Real change updates both float-bit states then color.
        backend.events.clear();
        state.curr_fog_color = 0;
        set_fog(&mut state, 1, FOGTYPE_LINEAR, 2.0, 10.0, 0x11, &mut backend);
        assert_eq!(state.curr_fog_end, 10.0);
        assert_eq!(state.curr_fog_start, 2.0);
        assert_eq!(
            backend.events,
            vec![
                format!("end:{:#x}", 10.0f32.to_bits()),
                format!("start:{:#x}", 2.0f32.to_bits()),
                "color:0x11".to_string(),
            ]
        );
    }

    #[test]
    fn fog_exp_uses_density_and_table_select() {
        let mut state = RenderState {
            hardware_caps: HWCAPS_TABLEFOG,
            curr_fog_state: 0,
            ..Default::default()
        };
        let mut backend = FogLog { events: Vec::new() };
        set_fog(&mut state, 1, FOGTYPE_EXP, 0.5, 0.0, 0x0, &mut backend);
        assert_eq!(state.curr_fog_type, FOGTYPE_EXP);
        assert!(backend.events.contains(&"enable:true".to_string()));
        assert!(backend.events.contains(&"mode:true:1".to_string()));
        assert!(backend
            .events
            .contains(&format!("density:{:#x}", 0.5f32.to_bits())));
    }

    #[test]
    fn fog_linear_maps_to_d3d_word_not_selector() {
        // FOGTYPE_LINEAR (0) must program D3DFOG_LINEAR (3): forwarding the
        // selector raw would program D3DFOG_NONE (fog off).
        let mut state = RenderState {
            hardware_caps: HWCAPS_TABLEFOG,
            curr_fog_state: 0,
            curr_fog_type: FOGTYPE_EXP, // force a type transition below
            ..Default::default()
        };
        let mut backend = FogLog { events: Vec::new() };
        set_fog(&mut state, 1, FOGTYPE_LINEAR, 1.0, 9.0, 0x0, &mut backend);
        assert!(backend.events.contains(&"mode:true:3".to_string()));
        let mut state = RenderState {
            curr_fog_state: 0,
            curr_fog_type: FOGTYPE_EXP,
            ..Default::default()
        };
        let mut backend = FogLog { events: Vec::new() };
        set_fog(&mut state, 1, FOGTYPE_LINEAR, 1.0, 9.0, 0x0, &mut backend);
        assert!(backend.events.contains(&"mode:false:3".to_string()));
    }

    struct AlphaLog {
        events: Vec<String>,
    }
    impl AlphaStateBackend for AlphaLog {
        fn set_alpha_blend_enable(&mut self, enable: bool) {
            self.events.push(format!("blend:{enable}"));
        }
        fn set_alpha_test_enable(&mut self, enable: bool) {
            self.events.push(format!("test:{enable}"));
        }
        fn set_alpha_ref(&mut self, value: u32) {
            self.events.push(format!("ref:{value}"));
        }
    }

    #[test]
    fn alpha_caches_and_maps_all_modes_with_store_before_switch() {
        let mut state = RenderState::default();
        let mut backend = AlphaLog { events: Vec::new() };
        state.curr_alpha_state = ALPHASTATE_NONE;
        set_alpha_state(&mut state, ALPHASTATE_NONE, &mut backend);
        assert!(backend.events.is_empty());
        state.curr_alpha_state = 99;
        backend.events.clear();
        set_alpha_state(&mut state, ALPHASTATE_NONE, &mut backend);
        assert_eq!(state.curr_alpha_state, ALPHASTATE_NONE);
        assert_eq!(backend.events, vec!["blend:false", "test:false"]);
        state.curr_alpha_state = 99;
        backend.events.clear();
        set_alpha_state(&mut state, ALPHASTATE_BLEND, &mut backend);
        assert_eq!(backend.events, vec!["blend:true", "test:true", "ref:1"]);
        state.curr_alpha_state = 99;
        state.alpha_ref = 0x20;
        backend.events.clear();
        set_alpha_state(&mut state, ALPHASTATE_TEST, &mut backend);
        assert_eq!(backend.events, vec!["blend:false", "test:true", "ref:32"]);
        state.curr_alpha_state = 99;
        backend.events.clear();
        set_alpha_state(&mut state, ALPHASTATE_BLEND_ALL, &mut backend);
        assert_eq!(backend.events, vec!["blend:true", "test:true", "ref:0"]);
        state.curr_alpha_state = 0;
        backend.events.clear();
        set_alpha_state(&mut state, 91, &mut backend);
        assert_eq!(state.curr_alpha_state, 91);
        assert!(backend.events.is_empty());
    }

    struct Stage0Log {
        ops: Vec<u32>,
    }
    impl ColorOpBackend for Stage0Log {
        fn set_color_op(&mut self, op: u32) {
            self.ops.push(op);
        }
    }
    impl AlphaOpBackend for Stage0Log {
        fn set_alpha_op(&mut self, op: u32) {
            self.ops.push(op);
        }
    }
    impl ColorOp1Backend for Stage0Log {
        fn set_color_op1(&mut self, op: u32) {
            self.ops.push(op);
        }
    }

    #[test]
    fn stage0_color_op_caches_then_forwards_every_op() {
        let mut state = RenderState {
            curr_color_op: 4,
            ..Default::default()
        };
        let mut backend = Stage0Log { ops: Vec::new() };
        set_color_op(&mut state, 4, &mut backend);
        assert!(backend.ops.is_empty());
        set_color_op(&mut state, 7, &mut backend);
        assert_eq!(state.curr_color_op, 7);
        assert_eq!(backend.ops, vec![7]);
        // Stage caches are independent: stage 1 holds its own word.
        set_color_op1(&mut state, 7, &mut backend);
        assert_eq!(state.curr_color_op1, 7);
        assert_eq!(state.curr_color_op, 7);
    }

    #[test]
    fn stage0_alpha_op_caches_then_forwards_every_op() {
        let mut state = RenderState {
            curr_alpha_op: 4,
            ..Default::default()
        };
        let mut backend = Stage0Log { ops: Vec::new() };
        set_alpha_op(&mut state, 4, &mut backend);
        assert!(backend.ops.is_empty());
        set_alpha_op(&mut state, 9, &mut backend);
        assert_eq!(state.curr_alpha_op, 9);
        assert_eq!(backend.ops, vec![9]);
    }
}

/// PC-only `I3D_driver` immediate debug-draw STUBs.
///
/// Evidenced names `I3D_driver::DrawPoint`, `DrawPoint2D`, `DrawLine`,
/// `DrawLines`, `DrawTriangles`, `DrawCircle2D`, `DrawText`, `DrawText2D`,
/// `DrawSprite`, `DrawBox`, `DrawSphere` from mafia-re
/// `ls3df/I3D/I3d_driver.cpp:728-1309` (helper `MakeColor` at :721-726, whose
/// pure color packing already lives in
/// `crate::presentation_material::pack_debug_draw_color`). Every RVA below is
/// a verified LS3DF code address: each has a `FUNCTION: LS3DF` marker in the
/// reference and was checked against both `docs/rewrite-overlay.json` and the
/// frozen `docs/progress.json` ledger (base-relative `RVA = VA - 0x10000000`,
/// so e.g. `0x166c0` is `VA 0x100166c0`); none is claimed in either file.
///
/// Shared native preamble (all draws): fog off (`set_fog` shape, see above),
/// clip on (`D3DRS_CLIPPING`, cached in `m_CurrClipState`), `D3DTOP_MODULATE`
/// color/alpha ops, the `IsAbsMrgZero(a - 1.0)` alpha pick (`ALPHASTATE_NONE`
/// when opaque, `ALPHASTATE_BLEND` otherwise; `set_alpha_state` shape above),
/// `BLENDSTATE_ALPHA`, texture stage 0 unbound, then the primitive. The
/// transparency convention is a byte: `a = (255 - (alpha & 0xff)) / 255`
/// packed by `MakeColor` (`FloatToInt` round-to-nearest, full-word shifts).
///
/// Every stub returns explicit `Unsupported`: the missing owners are the D3D8
/// device (`m_pD3DDevice8` draw/state calls), the shared `LVertList` /
/// `TLVertList` scratch, and — for the text/sprite/sphere draws — the font
/// material (`pFontMat`), the current camera (`pCurrentCamera`) and the
/// `IGraph` world-matrix sink. No pixels are produced and no render state is
/// touched; the doc-comments pin the native order so a future backend can
/// implement it without re-deriving the bodies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugDrawStubError {
    Unsupported { entry: u32 },
}

/// LS3DF RVA 0x166c0 (`VA 0x100166c0`).
pub const DRAW_POINT_ENTRY: u32 = 0x166c0;
/// LS3DF RVA 0x168f0 (`VA 0x100168f0`).
pub const DRAW_POINT_2D_ENTRY: u32 = 0x168f0;
/// LS3DF RVA 0x16b20 (`VA 0x10016b20`).
pub const DRAW_LINE_ENTRY: u32 = 0x16b20;
/// LS3DF RVA 0x16d70 (`VA 0x10016d70`).
pub const DRAW_LINES_ENTRY: u32 = 0x16d70;
/// LS3DF RVA 0x16ff0 (`VA 0x10016ff0`).
pub const DRAW_TRIANGLES_ENTRY: u32 = 0x16ff0;
/// LS3DF RVA 0x17580 (`VA 0x10017580`).
pub const DRAW_CIRCLE_2D_ENTRY: u32 = 0x17580;
/// LS3DF RVA 0x17890 (`VA 0x10017890`).
pub const DRAW_TEXT_ENTRY: u32 = 0x17890;
/// LS3DF RVA 0x17d40 (`VA 0x10017d40`).
pub const DRAW_TEXT_2D_ENTRY: u32 = 0x17d40;
/// LS3DF RVA 0x182f0 (`VA 0x100182f0`).
pub const DRAW_SPRITE_ENTRY: u32 = 0x182f0;
/// LS3DF RVA 0x186a0 (`VA 0x100186a0`).
pub const DRAW_BOX_ENTRY: u32 = 0x186a0;
/// LS3DF RVA 0x18cc0 (`VA 0x10018cc0`).
pub const DRAW_SPHERE_ENTRY: u32 = 0x18cc0;

/// LS3DF RVA 0x166c0 (`VA 0x100166c0`) `I3D_driver::DrawPoint` (mafia-re
/// `ls3df/I3D/I3d_driver.cpp:730-754`): shared preamble, then one `LVERTEX`
/// at `pos` with the packed color, `VF_LVERTEX`, `D3DPT_POINTLIST` of 1.
/// D3D8 device owner missing.
pub fn driver_draw_point(
    _pos: [f32; 3],
    _color: [f32; 3],
    _alpha: u32,
) -> Result<(), DebugDrawStubError> {
    Err(DebugDrawStubError::Unsupported {
        entry: DRAW_POINT_ENTRY,
    })
}

/// LS3DF RVA 0x168f0 (`VA 0x100168f0`) `I3D_driver::DrawPoint2D` (mafia-re
/// `ls3df/I3D/I3d_driver.cpp:758-784`): shared preamble, then one `TLVERTEX`
/// at `(x, y, 0)` with `rhw 1`, packed color, `specular 0`, `VF_TLVERTEX`,
/// `D3DPT_POINTLIST` of 1. D3D8 device owner missing.
pub fn driver_draw_point_2d(
    _x: i32,
    _y: i32,
    _color: [f32; 3],
    _alpha: u32,
) -> Result<(), DebugDrawStubError> {
    Err(DebugDrawStubError::Unsupported {
        entry: DRAW_POINT_2D_ENTRY,
    })
}

/// LS3DF RVA 0x16b20 (`VA 0x10016b20`) `I3D_driver::DrawLine` (mafia-re
/// `ls3df/I3D/I3d_driver.cpp:788-816`): shared preamble, then two `LVERTEX`
/// at `from`/`to` with the packed color, `VF_LVERTEX`, `D3DPT_LINELIST` of 1.
/// D3D8 device owner missing.
pub fn driver_draw_line(
    _from: [f32; 3],
    _to: [f32; 3],
    _color: [f32; 3],
    _alpha: u32,
) -> Result<(), DebugDrawStubError> {
    Err(DebugDrawStubError::Unsupported {
        entry: DRAW_LINE_ENTRY,
    })
}

/// LS3DF RVA 0x16d70 (`VA 0x10016d70`) `I3D_driver::DrawLines` (mafia-re
/// `ls3df/I3D/I3d_driver.cpp:820-852`): shared preamble, then the strided
/// vertex walk (`(uchar *)verts + i * vstride`) is fanned into the shared
/// `LVertList` with the packed color and drawn indexed (`D3DPT_LINELIST`,
/// `num_indices / 2`, 16-bit indices). D3D8 device and `LVertList` owners
/// missing.
pub fn driver_draw_lines(
    _num_verts: u32,
    _num_indices: u32,
    _color: [f32; 3],
    _alpha: u32,
) -> Result<(), DebugDrawStubError> {
    Err(DebugDrawStubError::Unsupported {
        entry: DRAW_LINES_ENTRY,
    })
}

/// LS3DF RVA 0x16ff0 (`VA 0x10016ff0`) `I3D_driver::DrawTriangles` (mafia-re
/// `ls3df/I3D/I3d_driver.cpp:868-966`): shared preamble with the transparency
/// taken from the flags low byte (`flags & I3DDRAW_TRANPARENCY_MASK`, `0xff`;
/// not the `alpha` low byte the other draws use), cull select (`I3DDRAW_NOCULL`
/// `0x200` forces none, else `I3DDRAW_CULL_CW` `0x400` picks CW, else CCW),
/// then the untransformed `LVERTEX` path or the `I3DDRAW_TRANSFORMED` `0x100`
/// `TLVERTEX` path (`rhw` from the fourth float, `specular 0`), each with or
/// without the parallel texcoord array (`has_texcoords`), drawn indexed
/// (`D3DPT_TRIANGLELIST`, `num_indices / 3`); the cull mode is restored to CCW
/// when forced-none or CW was used. Native returns `LS3D_RESULT`; D3D8 device
/// and `LVertList`/`TLVertList` owners missing.
pub fn driver_draw_triangles(
    _num_verts: u32,
    _num_indices: u32,
    _color: [f32; 3],
    _stride: u32,
    _flags: u32,
    _has_texcoords: bool,
) -> Result<(), DebugDrawStubError> {
    Err(DebugDrawStubError::Unsupported {
        entry: DRAW_TRIANGLES_ENTRY,
    })
}

/// LS3DF RVA 0x17580 (`VA 0x10017580`) `I3D_driver::DrawCircle2D` (mafia-re
/// `ls3df/I3D/I3d_driver.cpp:970-1006`): shared preamble, then `segments + 1`
/// `TLVERTEX` (`z 0`, `rhw 1`, packed color, `specular 0`) stepping the
/// `(0, -radius)` start vector by `RotateZ(2 * PI / segments)` around `center`
/// with the last vertex closed back onto the first, drawn as
/// `D3DPT_LINESTRIP` of `segments`. D3D8 device and `TLVertList` owners
/// missing.
pub fn driver_draw_circle_2d(
    _center: [f32; 2],
    _radius: f32,
    _color: [f32; 3],
    _segments: i32,
    _alpha: u32,
) -> Result<(), DebugDrawStubError> {
    Err(DebugDrawStubError::Unsupported {
        entry: DRAW_CIRCLE_2D_ENTRY,
    })
}

/// LS3DF RVA 0x17890 (`VA 0x10017890`) `I3D_driver::DrawText` (mafia-re
/// `ls3df/I3D/I3d_driver.cpp:1017-1091`): shared preamble plus `D3DCULL_NONE`,
/// early-out when no camera is current, world matrix at `pos` from the camera
/// matrix via `IGraph`, then per-character 6-vertex `LVERTEX` quads (`w = h =
/// size * 16`, `16 / font_width|height` UV cells) with the color packed from
/// the alpha byte and the *font material diffuse* (the draw takes no color
/// parameter), drawn as `D3DPT_TRIANGLELIST`; fill mode forced solid with the
/// wireframe-flag restore. Owners missing: camera, font material, `IGraph`
/// sink, D3D8 device, `LVertList`.
pub fn driver_draw_text(
    _pos: [f32; 3],
    _text: &str,
    _alpha: u8,
    _size: f32,
) -> Result<(), DebugDrawStubError> {
    Err(DebugDrawStubError::Unsupported {
        entry: DRAW_TEXT_ENTRY,
    })
}

/// LS3DF RVA 0x17d40 (`VA 0x10017d40`) `I3D_driver::DrawText2D` (mafia-re
/// `ls3df/I3D/I3d_driver.cpp:1098-1184`): shared preamble plus `D3DCULL_NONE`,
/// early-out when the font material is missing, then per-character quads at
/// `(x, y)` with `z 1`, `rhw 1` and zeroed `specular` (triangles `0 1 2` and
/// `3 2 1`) with the color packed from the alpha byte and the font material
/// diffuse, linear min/mag filtering with the `RS_LINEARFILTER` restore,
/// `ZENABLE` off/on around the draw and the wireframe-flag restore. Owners
/// missing: font material, D3D8 device, `TLVertList`.
pub fn driver_draw_text_2d(
    _x: i32,
    _y: i32,
    _text: &str,
    _alpha: u8,
    _size: f32,
) -> Result<(), DebugDrawStubError> {
    Err(DebugDrawStubError::Unsupported {
        entry: DRAW_TEXT_2D_ENTRY,
    })
}

/// The only `I3D_material` members `I3D_driver::DrawSprite` reads (mafia-re
/// `ls3df/I3D/I3d_driver.cpp:1192-1254`): `diffuse` feeds `MakeColor` with the
/// alpha byte; `flags` selects the alpha/blend pair (`MATF_ADD_MODE`
/// `0x80000000` → blend/add-alpha, `MATF_TXT_ALPHA` `0x40000000` → blend plus
/// add-alpha iff `emissive_nonzero` else plain alpha, `MATF_CKEY_ALPHA`
/// `0x20000000` → test/alpha, else none; words in `render_clip.rs`);
/// `has_texture` picks the material texture behind stage 0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DebugSpriteMaterial {
    pub diffuse: [f32; 3],
    pub flags: u32,
    pub emissive_nonzero: bool,
    pub has_texture: bool,
}

/// LS3DF RVA 0x182f0 (`VA 0x100182f0`) `I3D_driver::DrawSprite` (mafia-re
/// `ls3df/I3D/I3d_driver.cpp:1192-1254`): shared preamble, early-out when no
/// camera is current, world matrix at `pos` from the camera matrix via
/// `IGraph`, then the centered 4-corner `LVERTEX` fan (`±size / 2`, `.001` /
/// `.999` UVs) with the material-driven alpha/blend select above, drawn as
/// `D3DPT_TRIANGLESTRIP` of 2. Owners missing: camera, material/texture,
/// `IGraph` sink, D3D8 device.
pub fn driver_draw_sprite(
    _pos: [f32; 3],
    _material: &DebugSpriteMaterial,
    _alpha: u8,
    _size: f32,
) -> Result<(), DebugDrawStubError> {
    Err(DebugDrawStubError::Unsupported {
        entry: DRAW_SPRITE_ENTRY,
    })
}

/// LS3DF RVA 0x186a0 (`VA 0x100186a0`) `I3D_driver::DrawBox` (mafia-re
/// `ls3df/I3D/I3d_driver.cpp:1258-1273`): no-op unless the box is valid
/// (`min.x <= max.x`); otherwise the fill delegates to `DrawTriangles` over
/// the 8 box corners with the 36-index table, and the edges delegate to
/// `DrawLines` with the 24-index table and `alpha >> 8`. D3D8 device owner
/// missing (both delegates are stubs above).
pub fn driver_draw_box(
    _min: [f32; 3],
    _max: [f32; 3],
    _line_color: Option<[f32; 3]>,
    _fill_color: Option<[f32; 3]>,
    _alpha: u16,
) -> Result<(), DebugDrawStubError> {
    Err(DebugDrawStubError::Unsupported {
        entry: DRAW_BOX_ENTRY,
    })
}

/// LS3DF RVA 0x18cc0 (`VA 0x10018cc0`) `I3D_driver::DrawSphere` (mafia-re
/// `ls3df/I3D/I3d_driver.cpp:1298-1309`): no-op for `radius <= 0`; otherwise
/// the unit-ball wireframe (`VolumeBall`, 48 vertices on three circles,
/// `VolumeBallIndx` 96 indices) is scaled by `radius`, translated to `center`
/// and post-multiplied by `world` into the `IGraph` world matrix, then drawn
/// via `DrawLines` with `alpha >> 8`. Owners missing: `IGraph` sink, D3D8
/// device (the `DrawLines` delegate is a stub above).
pub fn driver_draw_sphere(
    _world: [[f32; 4]; 4],
    _center: [f32; 3],
    _radius: f32,
    _color: [f32; 3],
    _alpha: u16,
) -> Result<(), DebugDrawStubError> {
    Err(DebugDrawStubError::Unsupported {
        entry: DRAW_SPHERE_ENTRY,
    })
}

#[cfg(test)]
mod debug_draw_stub_tests {
    use super::*;

    #[test]
    fn stub_0x166c0_draw_point_is_unsupported() {
        assert_eq!(
            driver_draw_point([1.0, 2.0, 3.0], [1.0, 0.0, 0.0], 0),
            Err(DebugDrawStubError::Unsupported { entry: 0x166c0 })
        );
    }

    #[test]
    fn stub_0x168f0_draw_point_2d_is_unsupported() {
        assert_eq!(
            driver_draw_point_2d(4, 5, [0.0, 1.0, 0.0], 0),
            Err(DebugDrawStubError::Unsupported { entry: 0x168f0 })
        );
    }

    #[test]
    fn stub_0x16b20_draw_line_is_unsupported() {
        assert_eq!(
            driver_draw_line([0.0; 3], [1.0; 3], [0.0, 0.0, 1.0], 0),
            Err(DebugDrawStubError::Unsupported { entry: 0x16b20 })
        );
    }

    #[test]
    fn stub_0x16d70_draw_lines_is_unsupported() {
        assert_eq!(
            driver_draw_lines(8, 24, [1.0; 3], 0),
            Err(DebugDrawStubError::Unsupported { entry: 0x16d70 })
        );
    }

    #[test]
    fn stub_0x16ff0_draw_triangles_is_unsupported() {
        assert_eq!(
            driver_draw_triangles(8, 36, [1.0; 3], 12, 0, false),
            Err(DebugDrawStubError::Unsupported { entry: 0x16ff0 })
        );
    }

    #[test]
    fn stub_0x17580_draw_circle_2d_is_unsupported() {
        assert_eq!(
            driver_draw_circle_2d([0.0, 0.0], 4.0, [1.0; 3], 16, 0),
            Err(DebugDrawStubError::Unsupported { entry: 0x17580 })
        );
    }

    #[test]
    fn stub_0x17890_draw_text_is_unsupported() {
        assert_eq!(
            driver_draw_text([0.0; 3], "x", 0, 1.0),
            Err(DebugDrawStubError::Unsupported { entry: 0x17890 })
        );
    }

    #[test]
    fn stub_0x17d40_draw_text_2d_is_unsupported() {
        assert_eq!(
            driver_draw_text_2d(1, 2, "xy", 0, 1.0),
            Err(DebugDrawStubError::Unsupported { entry: 0x17d40 })
        );
    }

    #[test]
    fn stub_0x182f0_draw_sprite_is_unsupported() {
        let material = DebugSpriteMaterial {
            diffuse: [1.0; 3],
            flags: 0,
            emissive_nonzero: false,
            has_texture: false,
        };
        assert_eq!(
            driver_draw_sprite([0.0; 3], &material, 0, 2.0),
            Err(DebugDrawStubError::Unsupported { entry: 0x182f0 })
        );
    }

    #[test]
    fn stub_0x186a0_draw_box_is_unsupported() {
        assert_eq!(
            driver_draw_box([0.0; 3], [1.0; 3], None, Some([1.0; 3]), 0),
            Err(DebugDrawStubError::Unsupported { entry: 0x186a0 })
        );
    }

    #[test]
    fn stub_0x18cc0_draw_sphere_is_unsupported() {
        assert_eq!(
            driver_draw_sphere(
                [
                    [1.0, 0.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ],
                [0.0; 3],
                2.0,
                [1.0; 3],
                0
            ),
            Err(DebugDrawStubError::Unsupported { entry: 0x18cc0 })
        );
    }
}
