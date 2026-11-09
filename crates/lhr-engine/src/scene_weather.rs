//! Scene weather and sector sound-property defaults from original LS3DF.
//!
//! PC SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! Raw words preserve unknown fields without assigning guessed member names.
//! These operations do not construct a Scene, create Snow.tga or own resources.
#![forbid(unsafe_code)]

/// Native scene allocation size, 0x33c bytes, expressed as little-endian words.
pub const SCENE_WORDS: usize = 0x33c / 4;
/// Native S_sector_sound_properties record size, 0x68 bytes.
pub const SECTOR_SOUND_PROPERTIES_WORDS: usize = 26;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WeatherResetUnavailable;

/// Original WeatherSystemReset, PC RVA0x4b3b0, under Game's PC24/RN
/// arithmetic policy. Only incoming bits0 and6 influence the result. Unknown
/// words remain untouched. Numeric helpers admit the fixed native angle and
/// direction; their host availability error is not a native return code.
pub fn weather_system_reset(scene: &mut [u32; SCENE_WORDS]) -> Result<(), WeatherResetUnavailable> {
    use crate::native_precision::{add, div, mul, sqrt, sub};
    use crate::native_transform::{try_rounded_sin_cos, SMatrix};
    let input = scene[0x298 / 4];
    scene[0x298 / 4] = (input & 0x41) | 6;
    let parameters = if input & 0x40 != 0 {
        [0x3b03126f, 0x3e99999a, 0x3e19999a]
    } else {
        [0x3ccccccd, 0x3fc00000, 0x3cf5c28f]
    };
    scene[0x2ac / 4..0x2b8 / 4].copy_from_slice(&parameters);
    scene[0x2b8 / 4..0x2c8 / 4].copy_from_slice(&[25f32.to_bits(), 12f32.to_bits(), 1000, 3000]);
    let (sine, cosine) =
        try_rounded_sin_cos(f32::from_bits(0x3e4ccccd)).map_err(|_| WeatherResetUnavailable)?;
    let mut rotation = SMatrix::identity();
    rotation.elements[0] = cosine;
    rotation.elements[1] = sine;
    rotation.elements[4] = -sine;
    rotation.elements[5] = cosine;
    let direction = rotation
        .try_transform_point([0., -1., 0.])
        .map_err(|_| WeatherResetUnavailable)?;
    for offset in [0x2c8 / 4, 0x2d4 / 4] {
        scene[offset..offset + 3].copy_from_slice(&direction.map(f32::to_bits));
    }
    scene[0x320 / 4] = 0x80606060;
    scene[0x324 / 4] = 0x606060;

    // SetDir RVA0x2d9d0 on precisely the direction generated above. Its
    // general small-vector/fallback paths are not exposed as a separate port.
    let [dx, dy, dz] = direction;
    let squared = add(add(mul(dz, dz), mul(dy, dy)), mul(dx, dx));
    let mut matrix = SMatrix::identity();
    let z = if f64::from(sub(squared, 1.).abs()) < 1e-8 {
        direction
    } else {
        let reciprocal = div(1., sqrt(squared));
        direction.map(|component| mul(reciprocal, component))
    };
    matrix.elements[8..11].copy_from_slice(&z);
    let x_squared = add(mul(-dx, -dx), mul(dz, dz));
    let reciprocal = div(1., sqrt(x_squared));
    let x = [
        mul(reciprocal, dz),
        mul(reciprocal, 0.),
        mul(-dx, reciprocal),
    ];
    matrix.elements[..3].copy_from_slice(&x);
    let y = [
        -sub(mul(x[1], dz), mul(x[2], dy)),
        -sub(mul(x[2], dx), mul(dz, x[0])),
        -sub(mul(dy, x[0]), mul(x[1], dx)),
    ];
    matrix.elements[4..7].copy_from_slice(&y);
    scene[0x2e0 / 4..0x320 / 4].copy_from_slice(&matrix.elements.map(f32::to_bits));
    scene[0x2a4 / 4] = 0;
    scene[0x328 / 4] = sqrt(add(mul(25., 25.), mul(12., 12.))).to_bits();
    Ok(())
}

/// The ON branch of SetWeatherSystemParam, PC RVA0x4bce0. This is a
/// storage adapter, not the complete parameter dispatcher or particle update.
/// Only bit zero changes; turning weather off does not clear live particle data.
pub(crate) fn set_weather_enabled(scene: &mut [u32; SCENE_WORDS], enabled: bool) {
    const FLAGS: usize = 0x298 / 4;
    if enabled {
        scene[FLAGS] |= 1;
    } else {
        scene[FLAGS] &= !1;
    }
}

/// Original SetupDefaults__25S_sector_sound_propertiesFv (PS2 0x453910),
/// corresponding to PC RVA0x4e9b0. Replaces all 26 words; integer and
/// float fields retain their original representation without inferred labels.
pub fn setup_sector_sound_properties_defaults(record: &mut [u32; SECTOR_SOUND_PROPERTIES_WORDS]) {
    *record = [
        0x3f800000, 0x3f7d70a4, 0x5d2, 0x3ec4b73e, 0x3f2c2bf0, 7, 0x3f59999a, 10, 0x3f800000, 500,
        0x3ef286bd, 0, 250, 0, 250, 0, 0, 0x3f733333, 0x3e579436, 0x3e705398, 0, 0, 0, 0, 0, 0,
    ];
}

/// WS_PARAM discriminants, mafia-re ls3df/I3D/I3D.h:1326-1344 (working names).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum WsParam {
    On = 0,
    SectorTest = 1,
    DummyTest = 2,
    Color = 3,
    Color2 = 4,
    Density = 5,
    Size = 6,
    RangeX = 7,
    RangeY = 8,
    Speed = 9,
    Count = 10,
    WindX = 11,
    WindY = 12,
    WindZ = 13,
    Snow = 14,
    Mode = 15,
    Last = 16,
}
impl WsParam {
    pub fn from_word(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::On),
            1 => Some(Self::SectorTest),
            2 => Some(Self::DummyTest),
            3 => Some(Self::Color),
            4 => Some(Self::Color2),
            5 => Some(Self::Density),
            6 => Some(Self::Size),
            7 => Some(Self::RangeX),
            8 => Some(Self::RangeY),
            9 => Some(Self::Speed),
            10 => Some(Self::Count),
            11 => Some(Self::WindX),
            12 => Some(Self::WindY),
            13 => Some(Self::WindZ),
            14 => Some(Self::Snow),
            15 => Some(Self::Mode),
            _ => None,
        }
    }
}

pub const WSF_ON: u32 = 0x1;
pub const WSF_SECTOR_TEST: u32 = 0x2;
pub const WSF_DUMMY_TEST: u32 = 0x4;
pub const WSF_SNOW: u32 = 0x40;
pub const WSF_MODE_1: u32 = 0x200;
pub const WSF_MODE_2: u32 = 0x400;

const W_FLAGS: usize = 0x298 / 4;
const W_ELEMENTS_USED: usize = 0x2a4 / 4;
const W_DENSITY: usize = 0x2ac / 4;
const W_SIZE: usize = 0x2b0 / 4;
const W_SPEED: usize = 0x2b4 / 4;
const W_RANGE_X: usize = 0x2b8 / 4;
const W_RANGE_Y: usize = 0x2bc / 4;
const W_COUNT: usize = 0x2c0 / 4;
const W_WIND: usize = 0x2c8 / 4;
const W_WIND_RESET: usize = 0x2d4 / 4;
const W_COLOR: usize = 0x320 / 4;
const W_COLOR_2: usize = 0x324 / 4;
const W_RANGE: usize = 0x328 / 4;

/// Deferred original side effects of SetWeatherSystemParam, PC RVA 0x4bce0.
/// Word stores happen inline in call order; the caller performs the returned
/// effect through its own VB/particle backend (no live device here).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WeatherParamEffect {
    None,
    /// ws_count grew past the shared MaxWECnt: free WSComplVBStr (memset
    /// zero), record the grown max; the used_size truncation below already
    /// applied.
    FreeSharedVertexBuffer {
        grown_to: u32,
    },
    /// ws_color/ws_color_2 changed with a live VB: rewrite 12-vert colors.
    RewriteVertexColors,
    /// size/speed/snow changed: reshape every live particle's corners.
    ReshapeParticles,
}

/// Original SetWeatherSystemParam, PC RVA 0x4bce0 (contract: no Ghidra body;
/// dispatcher frame `SUB ESP,0xD8;CMP EAX,0xF` probed). `shared_max` is the
/// caller's MaxWECnt globals mirror (read+written only via the returned
/// effect discipline). Float scaling uses the PC24/RN f32 pipeline; the two
/// range recomputes use the original double-width sqrt narrowed once at
/// store. Unknown params are a silent no-op, as in the original default arm.
pub fn set_weather_system_param(
    scene: &mut [u32; SCENE_WORDS],
    param: u32,
    value: u32,
    shared_max: &mut u32,
) -> WeatherParamEffect {
    use crate::native_precision::mul;
    match param {
        0 => {
            if value != 0 {
                scene[W_FLAGS] |= WSF_ON;
            } else {
                scene[W_FLAGS] &= !WSF_ON;
            }
            WeatherParamEffect::None
        }
        1 => {
            if value != 0 {
                scene[W_FLAGS] |= WSF_SECTOR_TEST;
            } else {
                scene[W_FLAGS] &= !WSF_SECTOR_TEST;
            }
            WeatherParamEffect::None
        }
        2 => {
            if value != 0 {
                scene[W_FLAGS] |= WSF_DUMMY_TEST;
            } else {
                scene[W_FLAGS] &= !WSF_DUMMY_TEST;
            }
            WeatherParamEffect::None
        }
        3 => {
            scene[W_COLOR] = value;
            WeatherParamEffect::RewriteVertexColors
        }
        4 => {
            scene[W_COLOR_2] = value;
            WeatherParamEffect::RewriteVertexColors
        }
        5 => {
            scene[W_DENSITY] = mul(f32::from_bits(value), 0.001).to_bits();
            WeatherParamEffect::None
        }
        6 => {
            scene[W_SIZE] = value;
            WeatherParamEffect::ReshapeParticles
        }
        7 => {
            scene[W_RANGE_X] = value;
            let (x, y) = (
                f64::from(f32::from_bits(value)),
                f64::from(f32::from_bits(scene[W_RANGE_Y])),
            );
            scene[W_RANGE] = ((x * x + y * y).sqrt() as f32).to_bits();
            WeatherParamEffect::None
        }
        8 => {
            scene[W_RANGE_Y] = value;
            let (x, y) = (
                f64::from(f32::from_bits(scene[W_RANGE_X])),
                f64::from(f32::from_bits(value)),
            );
            scene[W_RANGE] = ((x * x + y * y).sqrt() as f32).to_bits();
            WeatherParamEffect::None
        }
        9 => {
            scene[W_SPEED] = mul(f32::from_bits(value), 0.5).to_bits();
            WeatherParamEffect::ReshapeParticles
        }
        10 => {
            scene[W_COUNT] = value;
            let mut effect = WeatherParamEffect::None;
            if *shared_max < value {
                *shared_max = value;
                effect = WeatherParamEffect::FreeSharedVertexBuffer { grown_to: value };
            }
            if scene[W_ELEMENTS_USED] > value {
                scene[W_ELEMENTS_USED] = value;
            }
            effect
        }
        11 => {
            scene[W_WIND] = value;
            WeatherParamEffect::None
        }
        12 => {
            scene[W_WIND + 1] = value;
            WeatherParamEffect::None
        }
        13 => {
            scene[W_WIND + 2] = value;
            WeatherParamEffect::None
        }
        14 => {
            if value == 1 {
                scene[W_FLAGS] |= WSF_SNOW;
            } else {
                scene[W_FLAGS] &= !WSF_SNOW;
            }
            WeatherParamEffect::ReshapeParticles
        }
        15 => {
            match value {
                2 => scene[W_FLAGS] |= WSF_MODE_2,
                1 => scene[W_FLAGS] |= WSF_MODE_1,
                _ => scene[W_FLAGS] &= !(WSF_MODE_1 | WSF_MODE_2),
            }
            WeatherParamEffect::None
        }
        _ => WeatherParamEffect::None,
    }
}

/// Original GetWeatherSystemParam, PC RVA 0x4c4d0 (contract: dispatcher
/// prologue `CMP EAX,0xF;JA` probed). Floats return as bits; unknown params
/// return (uint)-1.
pub fn get_weather_system_param(scene: &[u32; SCENE_WORDS], param: u32) -> u32 {
    use crate::native_precision::mul;
    match param {
        0 => scene[W_FLAGS] & WSF_ON,
        1 => scene[W_FLAGS] & WSF_SECTOR_TEST,
        2 => scene[W_FLAGS] & WSF_DUMMY_TEST,
        3 => scene[W_COLOR],
        4 => scene[W_COLOR_2],
        5 => mul(f32::from_bits(scene[W_DENSITY]), 1000.0).to_bits(),
        6 => scene[W_SIZE],
        7 => scene[W_RANGE_X],
        8 => scene[W_RANGE_Y],
        9 => mul(f32::from_bits(scene[W_SPEED]), 2.0).to_bits(),
        10 => scene[W_COUNT],
        11 => scene[W_WIND],
        12 => scene[W_WIND + 1],
        13 => scene[W_WIND + 2],
        14 => {
            if scene[W_FLAGS] & WSF_SNOW != 0 {
                1
            } else {
                0
            }
        }
        15 => {
            if scene[W_FLAGS] & WSF_MODE_1 != 0 {
                1
            } else if scene[W_FLAGS] & WSF_MODE_2 != 0 {
                2
            } else {
                0
            }
        }
        _ => u32::MAX,
    }
}

/// Working name for the WSF_ON bit test (mafia-re ls3df/I3D/I3D.h:1355-1356).
/// The reference records no LS3DF FUNCTION address for a standalone reader;
/// the bit is written by `SetWeatherSystemParam` (PC RVA 0x4bce0, already
/// occupied) and read back through `GetWeatherSystemParam` param 0. Reports
/// whether the scene weather system works; no state moves.
pub fn weather_is_on(flags: u32) -> bool {
    flags & WSF_ON != 0
}
/// Working name for the WSF_SECTOR_TEST bit test (mafia-re ls3df/I3D/I3D.h:1357).
/// The reference records no LS3DF FUNCTION address for a standalone reader.
/// Reports whether particles outside the primary sector stay hidden; no state
/// moves.
pub fn weather_tests_primary_sector(flags: u32) -> bool {
    flags & WSF_SECTOR_TEST != 0
}
/// Working name for the WSF_DUMMY_TEST bit test (mafia-re ls3df/I3D/I3D.h:1358).
/// The reference records no LS3DF FUNCTION address for a standalone reader.
/// Reports whether particles inside weather dummies' boxes stay hidden; no
/// state moves.
pub fn weather_tests_dummy_boxes(flags: u32) -> bool {
    flags & WSF_DUMMY_TEST != 0
}
/// Working name for the WSF_SNOW bit test (mafia-re ls3df/I3D/I3D.h:1359).
/// The reference records no LS3DF FUNCTION address for a standalone reader;
/// the bit is read back through `GetWeatherSystemParam` param 14. Reports
/// snow instead of rain; no state moves.
pub fn weather_is_snow(flags: u32) -> bool {
    flags & WSF_SNOW != 0
}
/// Working name for the `WS_PARAM_MODE` read arm of `GetWeatherSystemParam`
/// (mafia-re ls3df/I3D/I3d_scene.cpp:1307-1312). The reference records no
/// LS3DF FUNCTION address for a standalone reader; the arm lives inside the
/// occupied `GetWeatherSystemParam` dispatcher (PC RVA 0x4c4d0), whose other
/// arms already have standalone readers (`weather_is_on`,
/// `weather_tests_primary_sector`, `weather_tests_dummy_boxes`,
/// `weather_is_snow`) on the same precedent. `WSF_MODE_1` reports 1,
/// `WSF_MODE_2` reports 2, otherwise 0; no state moves.
pub fn weather_mode(flags: u32) -> u32 {
    if flags & WSF_MODE_1 != 0 {
        1
    } else if flags & WSF_MODE_2 != 0 {
        2
    } else {
        0
    }
}
/// Working name for the reset-wind snapshot read at +0x2d4 (mafia-re
/// ls3df/I3D/I3D.h:1599 `ws_dir`, the wind when it was reset). The
/// reference records no LS3DF FUNCTION address for a standalone reader:
/// the words are written by `WeatherSystemReset` alongside the live wind
/// (PC RVA 0x4b3b0, already occupied) and never through a getter. Reports
/// the stored words bit-exactly; no state moves.
pub fn weather_reset_wind(scene: &[u32; SCENE_WORDS]) -> [f32; 3] {
    [
        f32::from_bits(scene[W_WIND_RESET]),
        f32::from_bits(scene[W_WIND_RESET + 1]),
        f32::from_bits(scene[W_WIND_RESET + 2]),
    ]
}

/// WE_STRUCT corner reshaping shared by SetParam(update=2) and Update alloc,
/// RVAs 0x4bce0/0x4c660. `size/speed` are native f32 values, `angle` the
/// native rotation angle; rotation is RotateZMatrix(angle) via the rounded
/// sin/cos backend (failure is a host numeric unavailability, never native).
/// An exact-zero angle is the identity rotation (native RotateZMatrix(0)
/// admits all angles; the backend only covers nonzero |angle|<=1, disclosed).
pub fn reshape_particle_corners(
    size: f32,
    speed: f32,
    snow: bool,
    angle: f32,
) -> Result<[[f32; 3]; 4], WeatherResetUnavailable> {
    use crate::native_precision::div;
    use crate::native_transform::{try_rounded_sin_cos, SMatrix};
    let mut rotation = SMatrix::identity();
    if angle != 0.0 {
        let (sine, cosine) = try_rounded_sin_cos(angle).map_err(|_| WeatherResetUnavailable)?;
        rotation.elements[0] = cosine;
        rotation.elements[1] = sine;
        rotation.elements[4] = -sine;
        rotation.elements[5] = cosine;
    }
    let trailing = if snow { div(size, 2.0) } else { div(size, 6.0) };
    let leading = if snow { div(size, 2.0) } else { size };
    Ok([
        rotation.transform_point([0.0, 0.0, -leading]),
        rotation.transform_point([-speed, 0.0, 0.0]),
        rotation.transform_point([speed, 0.0, 0.0]),
        rotation.transform_point([0.0, 0.0, trailing]),
    ])
}

/// Pure CPU kernel of one m_UpdateWeatherSystem particle step, RVA 0x4c660
/// (contract: Ghidra body 1520 insn/173 BB with truncated value flow; only
/// this deterministic kernel is reconstructed). Random spawn draws are
/// caller-supplied (`rand3` = 3x RandFloat in call order); camera/sector/
/// dummy culling and VB emission stay with the caller. Snow sway uses
/// double-width sin narrowed once at store (x87 policy); the advance uses
/// the PC24 f32 pipeline. HIDDEN_MIN_Y forces a respawn next tick.
pub const HIDDEN_MIN_Y: f32 = 1_000_000.0;
pub struct ParticleStep {
    pub pos: [f32; 3],
    pub min_y: f32,
}
#[allow(clippy::too_many_arguments)]
pub fn step_weather_particle(
    pos: [f32; 3],
    min_y: f32,
    wind: [f32; 3],
    tick_delta_ms: u32,
    density: f32,
    cam_pos: [f32; 3],
    range: f32,
    range_x: f32,
    range_y: f32,
    snow: bool,
    phase: u32,
    tick_now_ms: u32,
    rand3: [f32; 3],
) -> ParticleStep {
    use crate::native_precision::{add, mul, sub};
    // Native association per component (I3d_scene.cpp:1462): ((wind * time)
    // * density), not wind * (time * density); the bits differ.
    let time_f32 = tick_delta_ms as f32;
    let mut next = [
        add(pos[0], mul(mul(wind[0], time_f32), density)),
        add(pos[1], mul(mul(wind[1], time_f32), density)),
        add(pos[2], mul(mul(wind[2], time_f32), density)),
    ];
    let dist_sq = |p: [f32; 3]| add(add(mul(p[0], p[0]), mul(p[1], p[1])), mul(p[2], p[2]));
    let rel = [
        sub(next[0], cam_pos[0]),
        sub(next[1], cam_pos[1]),
        sub(next[2], cam_pos[2]),
    ];
    let mut kept_min_y = min_y;
    if next[1] <= kept_min_y || dist_sq(rel) >= mul(range, range) {
        let span_x = add(range_x, range_x);
        let span_y = add(range_y, range_y);
        next = [
            add(sub(cam_pos[0], range_x), mul(rand3[0], span_x)),
            add(sub(cam_pos[1], range_y), mul(rand3[1], span_y)),
            add(sub(cam_pos[2], range_x), mul(rand3[2], span_x)),
        ];
        kept_min_y = sub(cam_pos[1], range_y);
    }
    if snow {
        let base = f64::from(phase.wrapping_add(tick_now_ms));
        next[0] -= ((base * 0.002).sin() * 0.004) as f32;
        next[2] += (((base + 500.0) * 0.002).sin() * 0.004) as f32;
    }
    ParticleStep {
        pos: next,
        min_y: kept_min_y,
    }
}

/// Fog-light selector for m_RenderWeatherSystem, RVA 0x4ddd0.
/// Types are I3DLIGHT_* (I3D.h:1860-1871): 5 FOG, 6 POINTAMBIENT, 7 POINTFOG.
pub struct WeatherFogLight {
    pub light_type: u32,
    pub range_n: f32,
    pub range_f: f32,
    pub near_range: f32,
    pub diffuse: [f32; 3],
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WeatherFogCmd {
    Linear {
        start_bits: u32,
        end_bits: u32,
        color: u32,
    },
    Exp {
        density_bits: u32,
        color: u32,
    },
    Exp2 {
        density_bits: u32,
        color: u32,
    },
}
fn fog_color(diffuse: [f32; 3]) -> u32 {
    // Native ROUND(diffuse*255) with current rounding (RN). PC24 pipeline
    // like the rest of the milestone. `as u32` saturates where C++
    // FloatToInt+shift wraps; saturation is unreachable on the admitted
    // domain (callers pass finite 0..1 diffuse — caller discipline, no
    // admission check here; out-of-domain callers diverge, disclosed).
    let channel = |v: f32| crate::native_precision::mul(v, 255.0).round_ties_even() as u32;
    0xff00_0000 | (channel(diffuse[0]) << 16) | (channel(diffuse[1]) << 8) | channel(diffuse[2])
}
/// Single ordered render command. Fixed states (Modulate2X color op,
/// MODULATE alpha, clipping/alpha/blend, ZWRITEENABLE FALSE->draw->TRUE,
/// CULL NONE, identity world, LVERTEX format, stream source) are
/// backend-owned and performed around exactly this draw; no live device is
/// touched here.
pub struct WeatherDrawCmd {
    pub snow_textured: bool,
    pub fog: Option<WeatherFogCmd>,
    pub base_vertex: u32,
    pub primitive_count: u32,
}
pub trait WeatherRenderBackend {
    fn draw_weather(&mut self, cmd: WeatherDrawCmd)
        -> Result<(), crate::model_loading::ModelError>;
}
/// Original m_RenderWeatherSystem, PC RVA 0x4ddd0 (contract: Ghidra body 409
/// insn/50 BB; ordered backend command, no live device). Returns drawn
/// primitives (0 = early-out with no backend call: `!count || !(flags&ON)`).
#[allow(clippy::too_many_arguments)]
pub fn render_weather_system(
    current_count: u32,
    flags: u32,
    fog_light: Option<&WeatherFogLight>,
    camera_fcp: f32,
    snow: bool,
    snow_texture_available: bool,
    base_vertex: u32,
    backend: &mut impl WeatherRenderBackend,
) -> Result<u32, crate::model_loading::ModelError> {
    if current_count == 0 || flags & WSF_ON == 0 {
        return Ok(0);
    }
    let fog = fog_light.and_then(|fog| match fog.light_type {
        5 => {
            let end = crate::native_precision::mul(camera_fcp, fog.range_f);
            Some(WeatherFogCmd::Linear {
                start_bits: crate::native_precision::mul(end, fog.range_n).to_bits(),
                end_bits: end.to_bits(),
                color: fog_color(fog.diffuse),
            })
        }
        6 => Some(WeatherFogCmd::Exp {
            density_bits: fog.near_range.to_bits(),
            color: fog_color(fog.diffuse),
        }),
        7 => Some(WeatherFogCmd::Exp2 {
            density_bits: fog.near_range.to_bits(),
            color: fog_color(fog.diffuse),
        }),
        _ => None,
    });
    backend.draw_weather(WeatherDrawCmd {
        snow_textured: snow && snow_texture_available,
        fog,
        base_vertex,
        primitive_count: current_count * 4,
    })?;
    Ok(current_count * 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weather_preserves_only_the_two_input_bits_and_unrelated_words() {
        for flags in [0, 1, 0x40, 0x41, 0xffff_ffbe, u32::MAX] {
            for incoming_precision in [false, true] {
                let mut state =
                    crate::native_precision::Pc24PrecisionState::new(incoming_precision);
                let mut scene = [0xdeadbeef; SCENE_WORDS];
                scene[0x298 / 4] = flags;
                state.run(|| weather_system_reset(&mut scene)).unwrap();
                assert_eq!(scene[0x298 / 4], (flags & 0x41) | 6);
                assert!(state.precision_raised());
                for (index, word) in scene.iter().enumerate() {
                    let offset = index * 4;
                    if offset != 0x298 && offset != 0x2a4 && !(0x2ac..=0x328).contains(&offset) {
                        assert_eq!(*word, 0xdeadbeef, "offset {offset:#x}");
                    }
                }
                assert_eq!(scene[0x2c8 / 4..0x2d4 / 4], scene[0x2d4 / 4..0x2e0 / 4]);
                assert_eq!(scene[0x2c0 / 4..0x2c8 / 4], [1000, 3000]);
                assert_eq!(scene[0x328 / 4], 769f32.sqrt().to_bits());
            }
        }
    }

    #[test]
    fn sound_defaults_overwrite_every_word_and_ignore_previous_contents() {
        let mut first = [0xdeadbeef; SECTOR_SOUND_PROPERTIES_WORDS];
        let mut second = [0; SECTOR_SOUND_PROPERTIES_WORDS];
        setup_sector_sound_properties_defaults(&mut first);
        setup_sector_sound_properties_defaults(&mut second);
        assert_eq!(first, second);
        assert_eq!(first[0], 1f32.to_bits());
        assert_eq!(first[2], 1490);
        assert_eq!(first[9], 500);
        assert_eq!(first[12..17], [250, 0, 250, 0, 0]);
        assert_eq!(first[20..], [0; 6]);
    }
    #[test]
    fn weather_on_bit_reports_working_system() {
        assert!(weather_is_on(WSF_ON));
        assert!(weather_is_on(u32::MAX));
        assert!(!weather_is_on(0));
        assert!(!weather_is_on(WSF_SECTOR_TEST));
    }
    #[test]
    fn weather_sector_test_bit_hides_outside_primary() {
        assert!(weather_tests_primary_sector(WSF_SECTOR_TEST));
        assert!(!weather_tests_primary_sector(WSF_ON));
        assert!(!weather_tests_primary_sector(0));
    }
    #[test]
    fn weather_dummy_test_bit_hides_inside_dummies() {
        assert!(weather_tests_dummy_boxes(WSF_DUMMY_TEST));
        assert!(!weather_tests_dummy_boxes(WSF_ON));
        assert!(!weather_tests_dummy_boxes(0));
    }
    #[test]
    fn weather_reset_wind_reads_the_reset_snapshot_words() {
        // Working name for ws_dir at +0x2d4 (I3D.h:1599): no LS3DF
        // FUNCTION address exists for a standalone reader.
        let scene = [0u32; SCENE_WORDS];
        assert_eq!(weather_reset_wind(&scene), [0., 0., 0.]);
        let mut state = crate::native_precision::Pc24PrecisionState::default();
        let mut reset = [0u32; SCENE_WORDS];
        state.run(|| weather_system_reset(&mut reset)).unwrap();
        let wind = weather_reset_wind(&reset);
        assert!(wind.iter().all(|v| v.is_finite()));
        assert_eq!(
            wind.map(f32::to_bits),
            [reset[W_WIND], reset[W_WIND + 1], reset[W_WIND + 2]]
        );
    }
    #[test]
    fn weather_mode_reports_mode_1_then_mode_2_then_none() {
        assert_eq!(weather_mode(0), 0);
        assert_eq!(weather_mode(WSF_ON), 0);
        assert_eq!(weather_mode(WSF_MODE_1), 1);
        assert_eq!(weather_mode(WSF_MODE_2), 2);
        // MODE_1 wins when both bits are set, exactly like the native arm order.
        assert_eq!(weather_mode(WSF_MODE_1 | WSF_MODE_2), 1);
        assert_eq!(weather_mode(WSF_MODE_1 | WSF_MODE_2 | WSF_SNOW), 1);
        assert_eq!(weather_mode(u32::MAX), 1);
    }
    #[test]
    fn weather_snow_bit_selects_snow() {
        assert!(weather_is_snow(WSF_SNOW));
        assert!(weather_is_snow(u32::MAX));
        assert!(!weather_is_snow(0));
        assert!(!weather_is_snow(WSF_ON | WSF_SECTOR_TEST | WSF_DUMMY_TEST));
    }
}

#[cfg(test)]
mod weather_system_tests {
    use super::*;

    #[test]
    fn set_get_param_roundtrip_covers_words_validation_and_effects() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut scene = [0u32; SCENE_WORDS];
            let mut max = 0u32;
            assert_eq!(
                set_weather_system_param(&mut scene, 0, 1, &mut max),
                WeatherParamEffect::None
            );
            assert_eq!(get_weather_system_param(&scene, 0), 1);
            assert_eq!(
                set_weather_system_param(&mut scene, 1, 7, &mut max),
                WeatherParamEffect::None
            );
            assert_eq!(get_weather_system_param(&scene, 1), 2);
            assert_eq!(
                set_weather_system_param(&mut scene, 0, 0, &mut max),
                WeatherParamEffect::None
            );
            assert_eq!(get_weather_system_param(&scene, 0), 0);
            assert_eq!(
                set_weather_system_param(&mut scene, 5, 0x4348_0000, &mut max),
                WeatherParamEffect::None
            );
            assert_eq!(get_weather_system_param(&scene, 5), 0x4348_0000);
            assert_eq!(
                set_weather_system_param(&mut scene, 6, 0x3fc0_0000, &mut max),
                WeatherParamEffect::ReshapeParticles
            );
            assert_eq!(get_weather_system_param(&scene, 6), 0x3fc0_0000);
            assert_eq!(
                set_weather_system_param(&mut scene, 7, 25f32.to_bits(), &mut max),
                WeatherParamEffect::None
            );
            assert_eq!(
                set_weather_system_param(&mut scene, 8, 12f32.to_bits(), &mut max),
                WeatherParamEffect::None
            );
            assert_eq!(scene[W_RANGE], (769f64.sqrt() as f32).to_bits());
            assert_eq!(
                set_weather_system_param(&mut scene, 9, 1f32.to_bits(), &mut max),
                WeatherParamEffect::ReshapeParticles
            );
            assert_eq!(get_weather_system_param(&scene, 9), 1f32.to_bits());
            scene[W_ELEMENTS_USED] = 500;
            assert_eq!(
                set_weather_system_param(&mut scene, 10, 1000, &mut max),
                WeatherParamEffect::FreeSharedVertexBuffer { grown_to: 1000 }
            );
            assert_eq!(max, 1000);
            assert_eq!(scene[W_COUNT], 1000);
            assert_eq!(scene[W_ELEMENTS_USED], 500);
            assert_eq!(
                set_weather_system_param(&mut scene, 10, 100, &mut max),
                WeatherParamEffect::None
            );
            assert_eq!(scene[W_ELEMENTS_USED], 100);
            assert_eq!(max, 1000);
            assert_eq!(
                set_weather_system_param(&mut scene, 14, 1, &mut max),
                WeatherParamEffect::ReshapeParticles
            );
            assert_eq!(get_weather_system_param(&scene, 14), 1);
            assert_eq!(
                set_weather_system_param(&mut scene, 15, 2, &mut max),
                WeatherParamEffect::None
            );
            assert_eq!(get_weather_system_param(&scene, 15), 2);
            assert_eq!(
                set_weather_system_param(&mut scene, 15, 9, &mut max),
                WeatherParamEffect::None
            );
            assert_eq!(get_weather_system_param(&scene, 15), 0);
            assert_eq!(WsParam::from_word(3), Some(WsParam::Color));
            assert_eq!(WsParam::from_word(16), None);
            assert_eq!(
                set_weather_system_param(&mut scene, 16, 1, &mut max),
                WeatherParamEffect::None
            );
            assert_eq!(get_weather_system_param(&scene, 16), u32::MAX);
            assert_eq!(get_weather_system_param(&scene, 99), u32::MAX);
        });
    }

    #[test]
    fn reshape_and_step_are_deterministic_with_sentinel_residue() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            // Exact-zero angle is the identity rotation with exact corners.
            let flat = reshape_particle_corners(1.5, 0.03, false, 0.0).unwrap();
            assert_eq!(
                flat.map(|corner| corner.map(f32::to_bits)),
                [
                    [0f32, 0., -1.5],
                    [-0.03, 0., 0.],
                    [0.03, 0., 0.],
                    [0., 0., 0.25]
                ]
                .map(|corner| corner.map(f32::to_bits))
            );
            // try_rounded_sin_cos admits nonzero |angle|<=1; rotation about z
            // preserves z bit-exactly and maps opposite corners to opposites.
            let rain = reshape_particle_corners(1.5, 0.03, false, 0.2).unwrap();
            assert_eq!(rain[0][2].to_bits(), (-1.5f32).to_bits());
            assert_eq!(rain[3][2].to_bits(), 0.25f32.to_bits());
            for (negated, corner) in rain[1].iter().zip(&rain[2]) {
                assert_eq!(negated + corner, 0.0);
            }
            let snow = reshape_particle_corners(0.3, 0.15, true, 0.2).unwrap();
            assert_eq!(snow[0][2].to_bits(), (-0.15f32).to_bits());
            assert_eq!(snow[3][2].to_bits(), 0.15f32.to_bits());
            for (negated, corner) in snow[1].iter().zip(&snow[2]) {
                assert_eq!(negated + corner, 0.0);
            }
            let args = |pos| {
                step_weather_particle(
                    pos,
                    -5.0,
                    [0.1, -1.0, 0.0],
                    16,
                    0.025,
                    [0.0, 0.0, 0.0],
                    27.0,
                    25.0,
                    12.0,
                    true,
                    12345,
                    100000,
                    [0.5, 0.5, 0.5],
                )
            };
            let (a, b) = (args([1.0, 2.0, 3.0]), args([1.0, 2.0, 3.0]));
            assert_eq!(a.pos.map(f32::to_bits), b.pos.map(f32::to_bits));
            assert_eq!(a.min_y.to_bits(), b.min_y.to_bits());
            // Snow sway moves x/z off the advected position.
            let plain = step_weather_particle(
                [1.0, 2.0, 3.0],
                -5.0,
                [0.1, -1.0, 0.0],
                16,
                0.025,
                [0.0, 0.0, 0.0],
                27.0,
                25.0,
                12.0,
                false,
                12345,
                100000,
                [0.5, 0.5, 0.5],
            );
            assert_ne!(a.pos.map(f32::to_bits), plain.pos.map(f32::to_bits));
            // Hidden sentinel forces respawn inside the box.
            let hidden = step_weather_particle(
                [50.0, 900.0, 50.0],
                HIDDEN_MIN_Y,
                [0.0, -1.0, 0.0],
                16,
                0.025,
                [0.0, 100.0, 0.0],
                27.0,
                25.0,
                12.0,
                false,
                0,
                0,
                [0.5, 0.5, 0.5],
            );
            assert_eq!(hidden.pos, [0.0, 100.0, 0.0]);
            assert_eq!(hidden.min_y, 88.0);
        });
    }

    #[test]
    fn render_early_out_emits_nothing_and_draw_is_ordered() {
        struct Log(Vec<WeatherDrawCmd>);
        impl WeatherRenderBackend for Log {
            fn draw_weather(
                &mut self,
                cmd: WeatherDrawCmd,
            ) -> Result<(), crate::model_loading::ModelError> {
                self.0.push(cmd);
                Ok(())
            }
        }
        let mut log = Log(Vec::new());
        assert_eq!(
            render_weather_system(0, 1, None, 10000.0, false, false, 0, &mut log).unwrap(),
            0
        );
        assert_eq!(
            render_weather_system(10, 0, None, 10000.0, false, false, 0, &mut log).unwrap(),
            0
        );
        assert!(log.0.is_empty());
        let fog = WeatherFogLight {
            light_type: 5,
            range_n: 0.5,
            range_f: 0.02,
            near_range: 0.0,
            diffuse: [1.0, 0.5, 0.25],
        };
        assert_eq!(
            render_weather_system(3, 1, Some(&fog), 10000.0, true, true, 48, &mut log).unwrap(),
            12
        );
        let cmd = log.0.pop().unwrap();
        assert!(cmd.snow_textured);
        assert_eq!((cmd.base_vertex, cmd.primitive_count), (48, 12));
        assert!(matches!(cmd.fog, Some(WeatherFogCmd::Linear { .. })));
    }
}
