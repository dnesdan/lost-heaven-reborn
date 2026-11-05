#![forbid(unsafe_code)]
//! I3D_driver lifecycle: Tick, GetTickTime, Init, Close, GetStats (LS3DF.dll
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6).
//! Upstream reference mafia-re ls3df/I3D/I3d_driver.cpp @ ab67b13 (Tick 171-174,
//! GetTickTime 178-181, Init 183-241, Close 247-262, GetStats 266-290), render
//! state enum I3D.h:107-185, driver members I3D.h:4403-4415, result codes
//! ls3df.h:20-35. Names Tick/GetTickTime/Init/Close/GetStats are evidenced PS2
//! originals (I3D_driver vtable); Rust spelling is snake_case per repo rule.
//! Tier contract (new-module wave policy: contract regardless of bodies;
//! GetStats formed 7BB/26insn/77B per assignment, the rest LAB_ entry shapes).
//!
//! Typed live state: one driver owner holds the tick word, the 63 render-state
//! words, f32 LOD words, bias words, init/disable flags, fast-find roots and
//! borrowed material identities. Byte buffers are not game objects here; the
//! only arrays are the fixed native banks ([i32; 63], [u32; 8]).
//! Precision policy: LOD words are f32 bit copies (native I3DFloatAsInt stores
//! the float bits with no arithmetic); no host f64 or fused ops.
//! Renderer/material effects (closeRender/initRender/GetIcon/Release) and the
//! GetStats diagnostic text stay behind backend traits; the engine never
//! formats text itself. Reread order follows the original: Close() runs inside
//! Init before I3DIsInit is set, closeRender runs first inside Close.

use crate::frame_transform::FrameKey;
use std::sync::atomic::{AtomicU64, Ordering};

/// LS3D_RESULT words evidenced at GetStats (I3D_OK/INVALIDPARAMS/UNSUPPORTED).
pub const LIFECYCLE_OK: u32 = 0;
pub const LIFECYCLE_INVALIDPARAMS: u32 = 0x8000_0007;
pub const LIFECYCLE_UNSUPPORTED: u32 = 0x8000_000e;

/// Render-state indices touched by Init (I3D.h:107-185).
pub const RS_LINEARFILTER: usize = 0;
pub const RS_UNKNOWN_1: usize = 1;
pub const RS_MIPMAP: usize = 2;
pub const RS_USEZB: usize = 3;
pub const RS_CLEAR: usize = 5;
pub const RS_CLEAR_ZBUFFER: usize = 6;
pub const RS_DITHER: usize = 7;
pub const RS_LOADMIPMAP: usize = 13;
pub const RS_TEXTUREDITHER: usize = 16;
pub const RS_DRAWVISUALS: usize = 18;
pub const RS_FOG: usize = 19;
pub const RS_LOD_QUALITY: usize = 20;
pub const RS_TEXTURECOMPRESS: usize = 25;
pub const RS_UNKNOWN_26: usize = 26;
pub const RS_UNKNOWN_32: usize = 32;
pub const RS_LMTRUECOLOR: usize = 34;
pub const RS_LMDITHER: usize = 35;
pub const RS_LOD_INDEX: usize = 38;
pub const RS_USE_OCCLUSION: usize = 39;
pub const RS_LM_AA_RATIO: usize = 40;
pub const RS_USESHADOWS: usize = 43;
pub const RS_UNKNOWN_45: usize = 45;
pub const RS_SHADOW_SW: usize = 48;
pub const RS_FAST_FIND: usize = 49;
pub const RS_LOD_SCALE: usize = 50;
pub const RS_USEPROJECTORS: usize = 51;
pub const RS_NONORTHO_TESSELLATION: usize = 53;
pub const RS_ZBIAS_PROJECTOR: usize = 54;
pub const RS_ZBIAS_SHADOW: usize = 55;
pub const RS_DRAWMIRRORS: usize = 59;
pub const RS_LIFT_PROJECTOR: usize = 60;
pub const RS_LIFT_SHADOW: usize = 61;
pub const RS_ENVMAP_SCALE: usize = 62;
pub const RS_LAST: usize = 63;

/// GetStats indices (I3D.h volume-first enum).
pub const STATS_VOLUME: u32 = 0;
pub const STATS_RENDER: u32 = 1;
pub const STATS_SCENE: u32 = 2;
pub const STATS_VIDEO: u32 = 3;
pub const STATS_MEMORY: u32 = 4;
pub const STATS_SOUND: u32 = 5;
pub const STATS_BSP: u32 = 6;

/// Hardware problem flags read by Init (I3D.h:4436-4445).
pub const PROBLEM_NO_FOG: u32 = 0x80;
pub const PROBLEM_ZBIAS_SCALE: u32 = 0x100;
/// Init-descriptor log flag tested by Init (IGraph.h:177).
pub const IGRAPH_LOG: u32 = 0x40;
/// D3DTEXF_LINEAR stored into RS_MIPMAP by Init.
pub const D3DTEXF_LINEAR: i32 = 2;
/// Bitmap id passed to GetIcon by Init.
pub const FONT_ICON_ID: u32 = 125;
/// Bit patterns of the f32 defaults (1.0f, 0.001f, 0.02f).
pub const ONE_FLOAT_BITS: u32 = 0x3f80_0000;
pub const LIFT_FLOAT_BITS: u32 = 0x3a83_126f;
pub const NONORTHO_FLOAT_BITS: u32 = 0x3ca3_d70a;

/// Borrowed material identity. The driver list borrows materials: release does
/// not destroy host material storage, it returns the identity to the backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DriverMaterialId {
    arena: u64,
    slot: u32,
}

impl DriverMaterialId {
    pub fn fresh() -> Self {
        static NEXT_MATERIAL: AtomicU64 = AtomicU64::new(1);
        let value = NEXT_MATERIAL
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .expect("driver material identity exhausted");
        Self {
            arena: 1,
            slot: u32::try_from(value).expect("driver material slot exhausted"),
        }
    }

    pub fn arena(self) -> u64 {
        self.arena
    }

    pub fn slot(self) -> u32 {
        self.slot
    }
}

/// Renderer/material backend. All methods are infallible: the original Init and
/// Close always return I3D_OK, and a missing font bitmap stores NULL.
pub trait DriverLifecycleBackend {
    fn close_render(&mut self);
    fn init_render(&mut self);
    fn get_icon(&mut self, res_id: u32) -> Option<DriverMaterialId>;
    fn release_material(&mut self, id: DriverMaterialId);
}

/// Diagnostic sink for the GetStats fallthrough message. The engine never
/// formats the invalid-index text itself.
pub trait DriverStatsDiagnostic {
    fn invalid_stats_index(&mut self, index: u32);
}

/// Typed driver globals. Construction is outside this contract (BSS zeros plus
/// the ctor-side emitor id owned elsewhere); Default matches the zeroed bank.
#[derive(Clone, Debug, PartialEq)]
pub struct DriverLifecycle {
    pub init_flags: u32,
    pub tick_time: i32,
    pub render_state: [i32; 63],
    pub lod_scale: f32,
    pub non_ortho_tessellation: f32,
    pub z_bias_shadow: i32,
    pub z_bias_projector: i32,
    pub is_init: bool,
    pub disable_log: bool,
    pub ff_root: Option<FrameKey>,
    pub ff_last_up: Option<FrameKey>,
    pub icon_materials: Vec<Option<DriverMaterialId>>,
    pub font_material: Option<DriverMaterialId>,
    pub volume_stats: [u32; 8],
}

impl Default for DriverLifecycle {
    fn default() -> Self {
        Self {
            init_flags: 0,
            tick_time: 0,
            render_state: [0; 63],
            lod_scale: 0.0,
            non_ortho_tessellation: 0.0,
            z_bias_shadow: 0,
            z_bias_projector: 0,
            is_init: false,
            disable_log: false,
            ff_root: None,
            ff_last_up: None,
            icon_materials: Vec::new(),
            font_material: None,
            volume_stats: [0; 8],
        }
    }
}

/// LS3DF RVA 0x15730. Evidenced name I3D_driver::Tick from mafia-re
/// I3d_driver.cpp:171-174. Native `driverTickTime += time` is an x86 wrapping
/// int add, preserved with wrapping_add.
pub fn driver_tick(state: &mut DriverLifecycle, time: i32) {
    state.tick_time = state.tick_time.wrapping_add(time);
}

/// LS3DF RVA 0x15750. Evidenced name I3D_driver::GetTickTime from mafia-re
/// I3d_driver.cpp:178-181. Pure load of the tick word; no mutation.
pub fn driver_get_tick_time(state: &DriverLifecycle) -> i32 {
    state.tick_time
}

/// LS3DF RVA 0x15760. Evidenced name I3D_driver::Init from mafia-re
/// I3d_driver.cpp:183-241. Effect order: init_flags store, tick reset, 63-word
/// defaults (RS_MIPMAP holds D3DTEXF_LINEAR, float words hold f32 bits,
/// RS_FOG conditional on PROBLEM_NO_FOG, bias prescale on PROBLEM_ZBIAS_SCALE
/// per the (value + 3) / 4 comment), LODScale/ZBias stores, Close() call,
/// I3DIsInit set, initRender, fast-find roots cleared, GetIcon(125),
/// disableLog set-only branch on missing IGRAPH_LOG. Always returns I3D_OK.
pub fn driver_init(
    state: &mut DriverLifecycle,
    flags: u32,
    problem_flags: u32,
    init_desc_flags: u32,
    backend: &mut impl DriverLifecycleBackend,
) -> u32 {
    state.init_flags = flags;
    state.tick_time = 0;
    state.render_state = [0; 63];
    state.render_state[RS_CLEAR] = 1;
    state.render_state[RS_CLEAR_ZBUFFER] = 1;
    state.render_state[RS_USEZB] = 1;
    state.render_state[RS_LOADMIPMAP] = 1;
    state.render_state[RS_DRAWVISUALS] = 1;
    state.render_state[RS_UNKNOWN_32] = 1;
    state.render_state[RS_UNKNOWN_26] = 1;
    state.render_state[RS_MIPMAP] = D3DTEXF_LINEAR;
    state.render_state[RS_LINEARFILTER] = 1;
    state.render_state[RS_UNKNOWN_1] = 1;
    state.render_state[RS_DITHER] = 1;
    state.render_state[RS_LMTRUECOLOR] = 1;
    state.render_state[RS_LMDITHER] = 1;
    state.render_state[RS_LM_AA_RATIO] = 1;
    state.render_state[RS_TEXTUREDITHER] = 1;
    state.render_state[RS_LOD_INDEX] = -1;
    state.render_state[RS_LOD_QUALITY] = ONE_FLOAT_BITS as i32;
    state.render_state[RS_USE_OCCLUSION] = 1;
    if problem_flags & PROBLEM_NO_FOG == 0 {
        state.render_state[RS_FOG] = 1;
    }
    state.render_state[RS_ENVMAP_SCALE] = ONE_FLOAT_BITS as i32;
    state.z_bias_projector = if problem_flags & PROBLEM_ZBIAS_SCALE != 0 {
        2
    } else {
        8
    };
    state.render_state[RS_LIFT_PROJECTOR] = LIFT_FLOAT_BITS as i32;
    state.render_state[RS_TEXTURECOMPRESS] = 1;
    state.render_state[RS_USESHADOWS] = 1;
    state.render_state[RS_UNKNOWN_45] = 1;
    state.render_state[RS_SHADOW_SW] = 1;
    state.render_state[RS_FAST_FIND] = 1;
    state.render_state[RS_LOD_SCALE] = 100;
    state.lod_scale = 1.0f32;
    state.render_state[RS_USEPROJECTORS] = 1;
    state.render_state[RS_NONORTHO_TESSELLATION] = 50;
    state.non_ortho_tessellation = 0.02f32;
    state.render_state[RS_ZBIAS_PROJECTOR] = 8;
    state.render_state[RS_DRAWMIRRORS] = 1;
    state.render_state[RS_ZBIAS_SHADOW] = 3;
    state.render_state[RS_LIFT_SHADOW] = LIFT_FLOAT_BITS as i32;
    state.z_bias_shadow = if problem_flags & PROBLEM_ZBIAS_SCALE != 0 {
        1
    } else {
        3
    };
    driver_close(state, backend);
    state.is_init = true;
    backend.init_render();
    state.ff_root = None;
    state.ff_last_up = None;
    state.font_material = backend.get_icon(FONT_ICON_ID);
    if init_desc_flags & IGRAPH_LOG == 0 {
        state.disable_log = true;
    }
    LIFECYCLE_OK
}

/// LS3DF RVA 0x15930. Evidenced name I3D_driver::Close from mafia-re
/// I3d_driver.cpp:247-262. Effect order: closeRender first, each icon entry
/// released then nulled in list order followed by clear, font released then
/// nulled, I3DIsInit cleared. Always returns I3D_OK.
pub fn driver_close(state: &mut DriverLifecycle, backend: &mut impl DriverLifecycleBackend) -> u32 {
    backend.close_render();
    for slot in state.icon_materials.iter_mut() {
        if let Some(id) = slot.take() {
            backend.release_material(id);
        }
    }
    state.icon_materials.clear();
    if let Some(id) = state.font_material.take() {
        backend.release_material(id);
    }
    state.is_init = false;
    LIFECYCLE_OK
}

/// LS3DF RVA 0x159b0. Evidenced name I3D_driver::GetStats from mafia-re
/// I3d_driver.cpp:266-290 (formed 7BB/26insn/77B per assignment). Null data
/// reports INVALIDPARAMS first; VOLUME copies 8 words and returns OK; the six
/// remaining indices return UNSUPPORTED without touching data or emitting the
/// diagnostic; any other index emits the diagnostic then INVALIDPARAMS.
pub fn driver_get_stats(
    state: &DriverLifecycle,
    index: u32,
    out: Option<&mut [u32; 8]>,
    diag: &mut impl DriverStatsDiagnostic,
) -> u32 {
    let Some(dst) = out else {
        return LIFECYCLE_INVALIDPARAMS;
    };
    match index {
        STATS_VOLUME => {
            dst.copy_from_slice(&state.volume_stats);
            LIFECYCLE_OK
        }
        STATS_RENDER | STATS_SCENE | STATS_VIDEO | STATS_MEMORY | STATS_SOUND | STATS_BSP => {
            LIFECYCLE_UNSUPPORTED
        }
        _ => {
            diag.invalid_stats_index(index);
            LIFECYCLE_INVALIDPARAMS
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct TestBackend {
        calls: Vec<String>,
        released: Vec<DriverMaterialId>,
        icon: Option<DriverMaterialId>,
    }

    impl DriverLifecycleBackend for TestBackend {
        fn close_render(&mut self) {
            self.calls.push("close_render".to_string());
        }

        fn init_render(&mut self) {
            self.calls.push("init_render".to_string());
        }

        fn get_icon(&mut self, res_id: u32) -> Option<DriverMaterialId> {
            self.calls.push(format!("get_icon:{res_id}"));
            self.icon
        }

        fn release_material(&mut self, id: DriverMaterialId) {
            self.calls.push(format!("release:{}", id.slot()));
            self.released.push(id);
        }
    }

    #[derive(Default)]
    struct TestDiag {
        invalid: Vec<u32>,
    }

    impl DriverStatsDiagnostic for TestDiag {
        fn invalid_stats_index(&mut self, index: u32) {
            self.invalid.push(index);
        }
    }

    #[test]
    fn tick_wraps_like_x86_int_add() {
        let mut state = DriverLifecycle::default();
        driver_tick(&mut state, 5);
        assert_eq!(state.tick_time, 5);
        driver_tick(&mut state, -8);
        assert_eq!(state.tick_time, -3);
        driver_tick(&mut state, i32::MAX);
        assert_eq!(state.tick_time, (-3i32).wrapping_add(i32::MAX));
        let mut overflow = DriverLifecycle::default();
        overflow.tick_time = i32::MAX;
        driver_tick(&mut overflow, 1);
        assert_eq!(overflow.tick_time, i32::MIN);
        let mut underflow = DriverLifecycle::default();
        underflow.tick_time = i32::MIN;
        driver_tick(&mut underflow, -1);
        assert_eq!(underflow.tick_time, i32::MAX);
    }

    #[test]
    fn get_tick_time_is_a_pure_load() {
        let mut state = DriverLifecycle::default();
        state.tick_time = -42;
        assert_eq!(driver_get_tick_time(&state), -42);
        assert_eq!(driver_get_tick_time(&state), -42);
        assert_eq!(state.tick_time, -42);
    }

    #[test]
    fn init_stores_flags_resets_tick_and_close_prefix_order() {
        let mut state = DriverLifecycle::default();
        state.tick_time = 999;
        state.render_state = [7; 63];
        let stale_icon = DriverMaterialId::fresh();
        let stale_font = DriverMaterialId::fresh();
        state.icon_materials = vec![Some(stale_icon), None, Some(stale_font)];
        state.font_material = Some(DriverMaterialId::fresh());
        state.ff_root = Some(FrameKey(3));
        state.ff_last_up = Some(FrameKey(4));
        let font = DriverMaterialId::fresh();
        let mut backend = TestBackend {
            icon: Some(font),
            ..TestBackend::default()
        };
        let result = driver_init(&mut state, 0x11, 0, 0, &mut backend);
        assert_eq!(result, LIFECYCLE_OK);
        assert_eq!(state.init_flags, 0x11);
        assert_eq!(state.tick_time, 0);
        assert!(state.is_init);
        assert_eq!(state.ff_root, None);
        assert_eq!(state.ff_last_up, None);
        assert_eq!(state.font_material, Some(font));
        assert!(state.icon_materials.is_empty());
        assert_eq!(state.lod_scale.to_bits(), ONE_FLOAT_BITS);
        assert_eq!(state.non_ortho_tessellation.to_bits(), NONORTHO_FLOAT_BITS);
        assert_eq!(state.z_bias_projector, 8);
        assert_eq!(state.z_bias_shadow, 3);
        assert_eq!(state.render_state[RS_FOG], 1);
        assert_eq!(state.render_state[RS_MIPMAP], 2);
        assert_eq!(state.render_state[RS_LOD_INDEX], -1);
        assert_eq!(state.render_state[RS_LOD_QUALITY], ONE_FLOAT_BITS as i32);
        assert_eq!(
            state.render_state[RS_LIFT_PROJECTOR],
            LIFT_FLOAT_BITS as i32
        );
        assert_eq!(state.render_state[RS_LOD_SCALE], 100);
        assert_eq!(state.render_state[RS_NONORTHO_TESSELLATION], 50);
        assert!(state.disable_log);
        assert_eq!(
            backend.calls.first(),
            Some(&"close_render".to_string()),
            "Close() prefix runs before initRender"
        );
        assert!(backend.calls.contains(&"init_render".to_string()));
        assert_eq!(backend.calls.last(), Some(&"get_icon:125".to_string()));
        assert_eq!(backend.released.len(), 3);
        assert!(backend.released.contains(&stale_icon));
        assert!(backend.released.contains(&stale_font));
    }

    #[test]
    fn init_problem_flags_and_sticky_disable_log() {
        let mut state = DriverLifecycle::default();
        let mut backend = TestBackend::default();
        driver_init(
            &mut state,
            0,
            PROBLEM_NO_FOG | PROBLEM_ZBIAS_SCALE,
            IGRAPH_LOG,
            &mut backend,
        );
        assert_eq!(state.render_state[RS_FOG], 0);
        assert_eq!(state.z_bias_projector, 2);
        assert_eq!(state.z_bias_shadow, 1);
        assert!(!state.disable_log);
        assert_eq!(state.font_material, None);
        driver_init(&mut state, 0, 0, 0, &mut backend);
        assert_eq!(state.render_state[RS_FOG], 1);
        assert!(state.disable_log);
        driver_init(&mut state, 0, 0, IGRAPH_LOG, &mut backend);
        assert!(state.disable_log, "disableLog is set-only");
    }

    #[test]
    fn close_releases_nulls_clears_in_order() {
        let mut state = DriverLifecycle::default();
        state.is_init = true;
        let first = DriverMaterialId::fresh();
        let second = DriverMaterialId::fresh();
        let font = DriverMaterialId::fresh();
        state.icon_materials = vec![Some(first), None, Some(second)];
        state.font_material = Some(font);
        let mut backend = TestBackend::default();
        assert_eq!(driver_close(&mut state, &mut backend), LIFECYCLE_OK);
        assert!(!state.is_init);
        assert!(state.icon_materials.is_empty());
        assert_eq!(state.font_material, None);
        assert_eq!(
            backend.calls,
            vec![
                "close_render".to_string(),
                format!("release:{}", first.slot()),
                format!("release:{}", second.slot()),
                format!("release:{}", font.slot()),
            ]
        );
        assert_eq!(driver_close(&mut state, &mut backend), LIFECYCLE_OK);
        assert_eq!(
            backend.calls.last(),
            Some(&"close_render".to_string()),
            "empty second close still closes render first"
        );
    }

    #[test]
    fn get_stats_null_volume_unsupported_and_fallthrough() {
        let mut state = DriverLifecycle::default();
        state.volume_stats = [11, 22, 33, 44, 55, 66, 77, 88];
        let mut diag = TestDiag::default();
        assert_eq!(
            driver_get_stats(&state, STATS_VOLUME, None, &mut diag),
            LIFECYCLE_INVALIDPARAMS
        );
        assert!(diag.invalid.is_empty());
        let mut out = [0u32; 8];
        assert_eq!(
            driver_get_stats(&state, STATS_VOLUME, Some(&mut out), &mut diag),
            LIFECYCLE_OK
        );
        assert_eq!(out, [11, 22, 33, 44, 55, 66, 77, 88]);
        for index in [
            STATS_RENDER,
            STATS_SCENE,
            STATS_VIDEO,
            STATS_MEMORY,
            STATS_SOUND,
            STATS_BSP,
        ] {
            let mut guard = [0xdead_beefu32; 8];
            assert_eq!(
                driver_get_stats(&state, index, Some(&mut guard), &mut diag),
                LIFECYCLE_UNSUPPORTED
            );
            assert_eq!(guard, [0xdead_beefu32; 8], "unsupported leaves data alone");
        }
        assert!(diag.invalid.is_empty());
        for index in [7u32, 8, 0x7fff_ffff, u32::MAX] {
            let mut guard = [1u32; 8];
            assert_eq!(
                driver_get_stats(&state, index, Some(&mut guard), &mut diag),
                LIFECYCLE_INVALIDPARAMS
            );
            assert_eq!(guard, [1u32; 8]);
        }
        assert_eq!(diag.invalid, vec![7, 8, 0x7fff_ffff, u32::MAX]);
    }
}
