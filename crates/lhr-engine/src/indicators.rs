//! PC `G_IndicatorsClass` HUD owner (PS2 name), marker-only contract scope.
//!
//! Game.exe SHA-256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27,
//! base 0x400000; addresses below are GAME RVAs. Reference: mafia-re 113b5b6
//! `game/ui/g_indicators.h:165` (class), `:199` (`show_flags` member offset
//! 0x40a4), `:31` (`INDF_*` bits), `:509` (`mafiaIndicators`); bodies at
//! `game/ui/g_indicators.cpp:118` (ctor, GAME 0x5f6b70), `:140` (`ResetValues`,
//! GAME 0x5f6cf0), `:256` (`Create`, GAME 0x5f70e0), `:298` (`Destroy`, GAME
//! 0x5f7250), `:355` (`GameInit`, GAME 0x5f73c0), `:391` (`GameDone`, GAME
//! 0x5f74a0), `:1432` (`DrawAll`, GAME 0x5fb060).
//!
//! Owned here: `show_flags` 0x40a4 + `show_flags_stack` 0x40a8 with the
//! `Set/Add/Remove/IsShown/Push/PopFlags` inline semantics (GAME
//! 0x47a070-0x47a2a0, `g_indicators.h:456-491`), `SetSymbol`/`SetProgress`
//! (GAME 0x47a300/0x47a340), the lifecycle (`constructed`/`create`/`destroy`/
//! `game_init` texture-plan + `game_done`/`reset_values`), the typed state
//! setters below, `TimerSetTime` (GAME 0x5f7500) / `TimerSetInterval` (GAME
//! 0x5f7540) / `TimerGetInterval` (GAME 0x5f7620), the pure layout math of
//! `ResolutionChanged` (GAME 0x601520, `g_indicators.cpp:3512`) over staged
//! screen dimensions, `SetRectVertices` (GAME 0x6025f0) /
//! `SetDiamondVertices` (GAME 0x6026d0) over the host vertex type, and the
//! needle math of `_SpocitejRafiku2` (GAME 0x5f7680,
//! `g_indicators.cpp:437`). `GetLanguage` (GAME 0x5f70d0,
//! `g_indicators.cpp:249`) is the stored primary-language code; the
//! `tables\menu\<name>_<lang>.mnu` file lookup lives in
//! `crate::original_language` (`GM_Menu::CreateCompsByDef`, GAME 0x5ea370).
//!
//! DEFERRED with explicit markers (no credit): `DrawAll` rendering and every
//! `igraph`/`ITexture` device effect (texture loads/releases, `LoadFont`
//! file reads at GAME 0x5f6f30, `LanguageSelect` file reads at GAME
//! 0x5f7000, `SetOverlayTexture` loads, viewport/scene work in
//! `ParheliaSetCenter`, widescreen/fade vertex and timing state machines,
//! blood/crew quads, radar road-edge and car-object payloads, console/
//! subtitle/mission-help/race text content and its text-database lookups).
//! Host float note: needle/layout math uses host `f32` `sin`/`cos`; last-ulp
//! differences against the original x87 path are possible and tests compare
//! with an epsilon. Integer/float conversions follow the native order for
//! finite in-range values; out-of-range host saturation is documented at
//! each site and never exercised by tests.

#![forbid(unsafe_code)]

use crate::game_init_order::{InitOperations, InitStep};
use crate::game_loadsave_lifecycle::LoadSaveEffects;

// INDF_* flag bits (`g_indicators.h:33-65`; the names are mafia-re working
// guesses, the values are evidenced).
pub const INDF_MENU: u32 = 1;
pub const INDF_TACHOMETR: u32 = 2;
pub const INDF_PLAYER: u32 = 4;
pub const INDF_TIMER: u32 = 8;
pub const INDF_PROGRESS: u32 = 0x10;
pub const INDF_CROSSHAIR: u32 = 0x20;
pub const INDF_COMPASS: u32 = 0x80;
pub const INDF_SPEED_LIMITER: u32 = 0x100;
pub const INDF_CUTSCENE: u32 = 0x800;
pub const INDF_THIEF: u32 = 0x1000;
pub const INDF_MAP: u32 = 0x2000;
pub const INDF_WIDESCREEN: u32 = 0x4000;
pub const INDF_FADE: u32 = 0x8000;
pub const INDF_CAR_DAMAGE: u32 = 0x10000;
pub const INDF_SNIPER: u32 = 0x20000;
pub const INDF_REFULLING: u32 = 0x40000;
pub const INDF_RADAR: u32 = 0x80000;
pub const INDF_RACE: u32 = 0x100000;
pub const INDF_RACE_START: u32 = 0x200000;
pub const INDF_RACE_FLASH: u32 = 0x400000;
pub const INDF_RACE_WRONG: u32 = 0x800000;
pub const INDF_MISSION_HELP: u32 = 0x1000000;
pub const INDF_CREW: u32 = 0x2000000;
pub const INDF_SYMBOL: u32 = 0x4000000;
pub const INDF_AMMO: u32 = 0x8000000;
pub const INDF_WINGMAN1: u32 = 0x10000000;
pub const INDF_WINGMAN2: u32 = 0x20000000;
pub const INDF_WINGMAN3: u32 = 0x40000000;
pub const INDF_WINGMAN4: u32 = 0x80000000;

/// `BonusSetCount` count that shows no bonus (`g_indicators.h:23`).
pub const BONUS_NONE: i32 = 0x7fffffff;
/// Wingmen count for `PlayerSetWingmanName/Lives` (`g_indicators.h:21`).
pub const MAX_WINGMEN: usize = 4;
/// HUD console ring length (`g_indicators.h:69`).
pub const CONSOLE_LINES: usize = 5;
/// `RaceSetStartFlag` hiding values (`g_indicators.h:73-74`).
pub const RACE_START_DONE: u32 = 4;
pub const RACE_START_NONE: u32 = 0xffffffff;

/// Staged wall-clock read for `ResetValues`/`create`. Native calls
/// `GetLocalTime` inside `ResetValues` (`g_indicators.cpp:162-164`); the host
/// stages the read and this owner applies it, preserving the call order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimerClock {
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

/// Host transformed vertex mirroring the `TLVERTEX` fields written by
/// `SetRectVertices`/`SetDiamondVertices`/`_SpocitejRafiku2`. Only the
/// written fields are modeled; the device buffer itself is deferred.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TlVertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rhw: f32,
    pub color: u32,
    pub u: f32,
    pub v: f32,
}

impl TlVertex {
    fn colored_point(x: f32, y: f32, color: u32, u: f32, v: f32) -> Self {
        Self {
            x,
            y,
            z: 0.0,
            rhw: 1.0,
            color,
            u,
            v,
        }
    }
}

/// A dial needle definition (`G_RafickaDef`, `g_indicators.h:109-119`; the
/// member names are mafia-re working guesses, the layout is evidenced).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NeedleDef {
    pub angle: f32,
    pub angle_range: f32,
    pub min: f32,
    pub range: f32,
    pub x: f32,
    pub y: f32,
    pub tip: f32,
    pub tail: f32,
    pub tip_width: f32,
    pub tail_width: f32,
    pub color: u32,
}

const fn degrees(value: f32) -> f32 {
    value * core::f32::consts::PI / 180.0
}

/// Native dial constants (`g_indicators.cpp:36-46`; `DEGREES` evaluated here
/// in host `f32`, see the module float note).
pub const TACHOMETER_DEF_SPEED: NeedleDef = NeedleDef {
    angle: degrees(207.0),
    angle_range: degrees(306.0),
    min: 0.0,
    range: 240.0,
    x: 90.5,
    y: 91.5,
    tip: 80.0,
    tail: -10.0,
    tip_width: 0.9,
    tail_width: 1.8,
    color: 0xff74130d,
};
pub const TACHOMETER_DEF_REVS: NeedleDef = NeedleDef {
    angle: degrees(200.0),
    angle_range: degrees(320.0),
    min: 0.0,
    range: 1.0,
    x: 37.5,
    y: 37.5,
    tip: 27.0,
    tail: -8.0,
    tip_width: 0.9,
    tail_width: 1.8,
    color: 0xff74130d,
};
pub const TACHOMETER_DEF_FUEL: NeedleDef = NeedleDef {
    angle: degrees(-45.0),
    angle_range: degrees(90.0),
    min: 0.0,
    range: 1.0,
    x: 90.5,
    y: 163.5,
    tip: 20.0,
    tail: -2.0,
    tip_width: 1.0,
    tail_width: 2.0,
    color: 0xff74130d,
};
pub const TIMER_DEF_MINUTES: NeedleDef = NeedleDef {
    angle: 0.0,
    angle_range: degrees(360.0),
    min: 0.0,
    range: 60.0,
    x: 42.0,
    y: 62.0,
    tip: 32.0,
    tail: -2.0,
    tip_width: 1.2,
    tail_width: 1.8,
    color: 0xff000000,
};
pub const TIMER_DEF_HOURS: NeedleDef = NeedleDef {
    angle: 0.0,
    angle_range: degrees(720.0),
    min: 0.0,
    range: 24.0,
    x: 42.0,
    y: 62.0,
    tip: 23.0,
    tail: -2.0,
    tip_width: 1.3,
    tail_width: 2.0,
    color: 0xff000000,
};
pub const TIMER_DEF_SECONDS: NeedleDef = NeedleDef {
    angle: 0.0,
    angle_range: degrees(360.0),
    min: 0.0,
    range: 60.0,
    x: 42.0,
    y: 80.5,
    tip: 10.0,
    tail: -5.0,
    tip_width: 0.9,
    tail_width: 1.4,
    color: 0xff000000,
};

/// Needle corners computed by `_SpocitejRafiku2` (`G_RafickaValue`
/// 0x4/0xc/0x14/0x1c, `g_indicators.h:123-130`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NeedleCorners {
    pub tail_left: [f32; 2],
    pub tip_left: [f32; 2],
    pub tail_right: [f32; 2],
    pub tip_right: [f32; 2],
}

/// `_SpocitejRafiku2` corner math (GAME 0x5f7680, `g_indicators.cpp:437-485`)
/// without the device vertex write; see `fill_needle_vertices` for that half.
/// `value` clamps to `[min, min + range]` exactly as the native `t` clamp.
pub fn compute_needle_corners(
    def: &NeedleDef,
    value: f32,
    pos: [f32; 2],
    hud_scale: [f32; 2],
) -> NeedleCorners {
    // All native defs have nonzero range; a zero range would divide by zero
    // exactly as the original does, so the caller stages evidenced defs.
    let mut t = (value - def.min) / def.range;
    if t < 0.0 {
        t = 0.0;
    } else if t > 1.0 {
        t = 1.0;
    }
    let angle = t * def.angle_range + def.angle;
    let (s, c) = angle.sin_cos();
    // Up at 0, clockwise on the screen (`g_indicators.cpp:448`).
    let dir = [s, -c];
    let side = [c, s];
    let corner = |along: f32, across: f32| {
        [
            (def.x + side[0] * across + dir[0] * along) * hud_scale[0] + pos[0],
            (def.y + side[1] * across + dir[1] * along) * hud_scale[1] + pos[1],
        ]
    };
    NeedleCorners {
        tip_right: corner(def.tip, def.tip_width),
        tail_right: corner(def.tail, def.tail_width),
        tip_left: corner(def.tip, -def.tip_width),
        tail_left: corner(def.tail, -def.tail_width),
    }
}

/// Device half of `_SpocitejRafiku2` (`g_indicators.cpp:455-484`): corner
/// order tail-left, tip-left, tail-right, tip-right with the (0,0)/(1,0)/
/// (0,1)/(1,1) texture coordinates, then `verts[4] = verts[2]` and
/// `verts[5] = verts[1]`.
pub fn fill_needle_vertices(corners: &NeedleCorners, color: u32) -> [TlVertex; 6] {
    let verts = [
        TlVertex::colored_point(corners.tail_left[0], corners.tail_left[1], color, 0.0, 0.0),
        TlVertex::colored_point(corners.tip_left[0], corners.tip_left[1], color, 1.0, 0.0),
        TlVertex::colored_point(corners.tail_right[0], corners.tail_right[1], color, 0.0, 1.0),
        TlVertex::colored_point(corners.tip_right[0], corners.tip_right[1], color, 1.0, 1.0),
    ];
    [verts[0], verts[1], verts[2], verts[3], verts[2], verts[1]]
}

/// `SetRectVertices` (GAME 0x6025f0, `g_indicators.cpp:3599-3632`).
pub fn set_rect_vertices(
    pos: [f32; 2],
    size: [f32; 2],
    u0: f32,
    v0: f32,
    u1: f32,
    v1: f32,
    color: u32,
    verts: &mut [TlVertex; 6],
) {
    verts[0] = TlVertex::colored_point(pos[0], pos[1], color, u0, v0);
    verts[1] = TlVertex::colored_point(pos[0] + size[0], pos[1], color, u1, v0);
    verts[2] = TlVertex::colored_point(pos[0], pos[1] + size[1], color, u0, v1);
    verts[3] = TlVertex::colored_point(pos[0] + size[0], pos[1] + size[1], color, u1, v1);
    verts[4] = verts[2];
    verts[5] = verts[1];
}

/// `SetDiamondVertices` (GAME 0x6026d0, `g_indicators.cpp:3635-3668`).
pub fn set_diamond_vertices(
    pos: [f32; 2],
    size: [f32; 2],
    u0: f32,
    v0: f32,
    u1: f32,
    v1: f32,
    color: u32,
    verts: &mut [TlVertex; 6],
) {
    verts[0] = TlVertex::colored_point(pos[0] - size[0], pos[1], color, u0, v0);
    verts[1] = TlVertex::colored_point(pos[0], pos[1] - size[1], color, u1, v0);
    verts[2] = TlVertex::colored_point(pos[0], pos[1] + size[1], color, u0, v1);
    verts[3] = TlVertex::colored_point(pos[0] + size[0], pos[1], color, u1, v1);
    verts[4] = verts[2];
    verts[5] = verts[1];
}

/// `PARHELIA_*` layout (`g_indicators.h:28-30`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParheliaLayout {
    None,
    Dual,
    Triple,
}

impl ParheliaLayout {
    pub fn index(self) -> i32 {
        match self {
            ParheliaLayout::None => 0,
            ParheliaLayout::Dual => 1,
            ParheliaLayout::Triple => 2,
        }
    }
}

/// Pure `GameInit` decision (GAME 0x5f73c0, `g_indicators.cpp:355-387`): the
/// dial/icon asset names selected from the night-mission flag and the
/// profile speedometer unit. The `igraph->CreateITexture` loads stay with
/// the device owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameInitTexturePlan {
    pub dial_asset: &'static str,
    pub icons_asset: &'static str,
}

/// One radar row. The car/object pointers (`C_car *`, `tDynamicCollObject *`)
/// stay with their owners; the host stages opaque ids only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RadarCar {
    pub car: u32,
    pub color: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RadarItem {
    pub object: u32,
    pub color: u32,
}

/// Which radar road edge is staged (`RadarSetLeftEdge` GAME 0x5fa9f0,
/// `RadarSetRightEdge` GAME 0x5fabf0; both deferred, vertex work included).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadarEdgeSide {
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
struct ConsoleLine {
    text: String,
    color: u32,
    time: f32,
}

impl Default for ConsoleLine {
    fn default() -> Self {
        Self {
            text: String::new(),
            color: 0,
            time: 0.0,
        }
    }
}

/// Owned HUD state. This is a typed owner, not a mirrored `0x4e38`-byte
/// layout: only the evidenced offsets named in the docs (`show_flags`
/// 0x40a4, `show_flags_stack` 0x40a8) keep their addresses as provenance.
#[derive(Clone, Debug, PartialEq)]
pub struct IndicatorsState {
    show_flags: u32,
    show_flags_stack: Vec<u32>,
    language: u32,
    created: bool,
    hud_textures_present: bool,
    dial_asset: Option<&'static str>,
    icons_asset: Option<&'static str>,
    map_texture_present: bool,
    fade_texture_present: bool,
    timer_hour: u32,
    timer_minute: u32,
    timer_second: u32,
    timer_interval_ms: u32,
    timer_interval_minute: f32,
    timer_interval_second: f32,
    timer_hands_invalid: bool,
    timer_hand_corners: [NeedleCorners; 3],
    tach_speed_value: f32,
    tach_revs_value: f32,
    tach_fuel_value: f32,
    tach_speed_corners: NeedleCorners,
    tach_revs_corners: NeedleCorners,
    tach_fuel_corners: NeedleCorners,
    display_column: f32,
    gear_column: f32,
    gearing_mode: bool,
    fuel_low: bool,
    fuel_light_phase: f32,
    car_damage: f32,
    lives: u32,
    lives_text: String,
    ammo: u32,
    ammo_spare: u32,
    ammo_text: String,
    accuracy: f32,
    panel_icon_alpha: u32,
    wingman_names: [u32; 4],
    wingman_lives: [u32; 4],
    compass_destination_present: bool,
    console_lines: [ConsoleLine; CONSOLE_LINES],
    console_first: u32,
    console_count: u32,
    thief_indicator: f32,
    refulling_indicator: f32,
    fade_color: u32,
    overlay_fade: f32,
    overlay_color: u32,
    overlay_name: Option<Vec<u8>>,
    overlay_texture_requested: bool,
    overlay_opaque: bool,
    radar_cars: Vec<RadarCar>,
    radar_player_car: Option<u32>,
    radar_items: Vec<RadarItem>,
    radar_unknown_44bc: u32,
    radar_scale: f32,
    radar_left_edge: Vec<[f32; 3]>,
    radar_right_edge: Vec<[f32; 3]>,
    race_start_flag_last: u32,
    race_start_flag: u32,
    race_start_last_time: f32,
    race_start_time: f32,
    race_time: String,
    race_lap_time: String,
    race_best_time: String,
    race_position: String,
    race_laps: String,
    race_flash_text: String,
    race_flash_alpha: f32,
    bonus_count: i32,
    bonus_text: Option<String>,
    play_time: u32,
    symbol_index: u32,
    symbol_color: i32,
    progress: f32,
    map_enabled: bool,
    parhelia_center: bool,
    parhelia_layout: ParheliaLayout,
    screen_size: [f32; 2],
    screen_scale: [f32; 2],
    hud_size: [f32; 2],
    hud_scale: [f32; 2],
    screen_edges: [[f32; 2]; 2],
    tachometer_pos: [f32; 2],
    revs_pos: [f32; 2],
    timer_pos: [f32; 2],
    screen_center: [f32; 2],
    widescreen_ratio: f32,
    crew_quads: u32,
}

fn zero_corners() -> NeedleCorners {
    NeedleCorners {
        tail_left: [0.0; 2],
        tip_left: [0.0; 2],
        tail_right: [0.0; 2],
        tip_right: [0.0; 2],
    }
}

impl IndicatorsState {
    /// Constructor (GAME 0x5f6b70, `g_indicators.cpp:118-136`): the `__idx`
    /// index-buffer loop is device vertex setup and stays deferred; the
    /// language tables zero, the font texture words NULL and `ResetValues`
    /// below are owned here with the staged clock.
    pub fn constructed(clock: TimerClock) -> Self {
        let mut state = Self {
            show_flags: 0,
            show_flags_stack: Vec::new(),
            language: 0,
            created: false,
            hud_textures_present: false,
            dial_asset: None,
            icons_asset: None,
            map_texture_present: false,
            fade_texture_present: false,
            timer_hour: 0,
            timer_minute: 0,
            timer_second: 0,
            timer_interval_ms: 0,
            timer_interval_minute: 0.0,
            timer_interval_second: 0.0,
            timer_hands_invalid: true,
            timer_hand_corners: [zero_corners(), zero_corners(), zero_corners()],
            tach_speed_value: 0.0,
            tach_revs_value: 0.0,
            tach_fuel_value: 0.0,
            tach_speed_corners: zero_corners(),
            tach_revs_corners: zero_corners(),
            tach_fuel_corners: zero_corners(),
            display_column: 0.0,
            gear_column: 0.0,
            gearing_mode: false,
            fuel_low: false,
            fuel_light_phase: 0.0,
            car_damage: 0.0,
            lives: 0,
            lives_text: String::new(),
            ammo: 0,
            ammo_spare: 0,
            ammo_text: String::new(),
            accuracy: 0.0,
            panel_icon_alpha: 0,
            wingman_names: [0; 4],
            wingman_lives: [0; 4],
            compass_destination_present: false,
            console_lines: Default::default(),
            console_first: 0,
            console_count: 0,
            thief_indicator: 0.0,
            refulling_indicator: 0.0,
            fade_color: 0,
            overlay_fade: 0.0,
            overlay_color: 0,
            overlay_name: None,
            overlay_texture_requested: false,
            overlay_opaque: false,
            radar_cars: Vec::new(),
            radar_player_car: None,
            radar_items: Vec::new(),
            radar_unknown_44bc: 0,
            radar_scale: 0.0,
            radar_left_edge: Vec::new(),
            radar_right_edge: Vec::new(),
            race_start_flag_last: 0,
            race_start_flag: RACE_START_NONE,
            race_start_last_time: 0.0,
            race_start_time: 0.0,
            race_time: String::new(),
            race_lap_time: String::new(),
            race_best_time: String::new(),
            race_position: String::new(),
            race_laps: String::new(),
            race_flash_text: String::new(),
            race_flash_alpha: 0.0,
            bonus_count: BONUS_NONE,
            bonus_text: None,
            play_time: 0,
            symbol_index: 0,
            symbol_color: 0,
            progress: 0.0,
            map_enabled: false,
            parhelia_center: false,
            parhelia_layout: ParheliaLayout::None,
            screen_size: [0.0; 2],
            screen_scale: [0.0; 2],
            hud_size: [0.0; 2],
            hud_scale: [0.0; 2],
            screen_edges: [[0.0; 2]; 2],
            tachometer_pos: [0.0; 2],
            revs_pos: [0.0; 2],
            timer_pos: [0.0; 2],
            screen_center: [0.0; 2],
            widescreen_ratio: 0.0,
            crew_quads: 0,
        };
        state.reset_values(clock);
        state
    }

    /// `ResetValues` (GAME 0x5f6cf0, `g_indicators.cpp:140-193`) in native
    /// order. Uninitialized-native fields (`car_damage`, `display_column`,
    /// `refulling_indicator`, `progress`, `panel_icon_alpha`, `crew_quads`,
    /// `overlay_color`) stay at the host zero from construction; the vertex
    /// `memset`s and `textDatabase.GetText(0x5f)` refresh are deferred to
    /// the device/text owners.
    pub fn reset_values(&mut self, clock: TimerClock) {
        self.show_flags = 0;
        self.show_flags_stack.clear();
        self.tachometr_set_gear(0, false);
        // Native passes 0 fuel with 0 revs, so `fuel_low` ends true and the
        // light phase stays 0; the odometer only feeds deferred digits.
        self.tachometr_set_values(0.0, 0.0, 0.0, 0.0);
        self.fuel_light_phase = 0.0;
        self.player_set_lives(100);
        self.player_set_ammo(16, 30);
        self.player_set_accuracy(1.0);
        for wingman in 0..MAX_WINGMEN {
            self.player_set_wingman_name(wingman as u32, 0);
            self.player_set_wingman_lives(wingman as u32, 100);
        }
        self.set_compass_destination(false);
        self.thief_indicator = 0.5;
        self.console_first = 0;
        self.console_count = 0;
        self.timer_set_time(clock.hour, clock.minute, clock.second);
        self.timer_set_interval(130);
        self.set_widescreen_ratio(0.5);
        self.radar_player_car = None;
        self.radar_cars.clear();
        self.radar_unknown_44bc = 0;
        self.radar_items.clear();
        self.radar_scale = 0.9;
        self.race_start_last_time = 0.0;
        self.race_start_time = 0.0;
        self.race_time.clear();
        self.race_lap_time.clear();
        self.race_best_time.clear();
        self.race_position.clear();
        self.race_laps.clear();
        self.play_time = 0;
        self.overlay_fade = 0.0;
        self.overlay_texture_requested = false;
        self.overlay_name = None;
        self.overlay_opaque = false;
        self.map_texture_present = false;
        self.fade_texture_present = false;
        self.symbol_index = 0;
        // Native stores -1 (`symbol_color = -1`, `g_indicators.cpp:187`).
        self.symbol_color = -1;
        self.map_enabled = false;
        self.parhelia_center = false;
        self.clear_intro_subtitle_slots();
        self.bonus_set_count(BONUS_NONE);
    }

    /// `Create` (GAME 0x5f70e0, `g_indicators.cpp:256-294`): `texture_map`
    /// cleared, `ResetValues` applied; every `igraph->CreateITexture` load
    /// and `LoadFont` file read stays with the device owner and is tracked
    /// only as presence.
    pub fn create(&mut self, clock: TimerClock) {
        self.map_texture_present = false;
        self.hud_textures_present = true;
        self.created = true;
        self.reset_values(clock);
    }

    /// `Destroy` (GAME 0x5f7250, `g_indicators.cpp:298-351`): releases what
    /// `Create` made, in native order; the actual `Release` calls stay with
    /// the device owner.
    pub fn destroy(&mut self) {
        self.hud_textures_present = false;
        self.dial_asset = None;
        self.icons_asset = None;
        self.map_texture_present = false;
        self.fade_texture_present = false;
        self.created = false;
    }

    /// Pure `GameInit` branch (GAME 0x5f73c0, `g_indicators.cpp:355-387`):
    /// night mission picks the `noc` dial/icon variants, the profile
    /// speedometer unit picks the `mph` dial variant.
    pub fn game_init_texture_plan(
        night_mission: bool,
        speedometer_mph: bool,
    ) -> GameInitTexturePlan {
        let dial_asset = match (night_mission, speedometer_mph) {
            (true, true) => "2intnocmph.tga",
            (true, false) => "2intnoc.tga",
            (false, true) => "2intmph.tga",
            (false, false) => "2int.tga",
        };
        let icons_asset = if night_mission {
            "2intauton.tga"
        } else {
            "2intauto.tga"
        };
        GameInitTexturePlan {
            dial_asset,
            icons_asset,
        }
    }

    /// Applies the `GameInit` plan; the texture release/load pair stays with
    /// the device owner.
    pub fn apply_game_init_texture_plan(&mut self, plan: GameInitTexturePlan) {
        self.dial_asset = Some(plan.dial_asset);
        self.icons_asset = Some(plan.icons_asset);
    }

    /// `GameDone` (GAME 0x5f74a0, `g_indicators.cpp:391-402`): map released,
    /// radar edges freed, bonus/intermission/race-start cleared,
    /// `TimerSetInterval(0)` re-applied from the current clock.
    pub fn game_done(&mut self) {
        self.map_texture_present = false;
        self.radar_left_edge.clear();
        self.radar_right_edge.clear();
        self.bonus_set_count(BONUS_NONE);
        self.clear_intro_subtitle_slots();
        self.race_set_start_flag(RACE_START_NONE);
        self.timer_set_interval(0);
    }

    /// `G_LoadSaveClass::Init` slot clear (GAME 0x604ee0,
    /// `g_loadsave.cpp:120`): a direct `show_flags = 0` store after
    /// `ResolutionChanged`, not a flag removal.
    pub fn clear_flags_slot(&mut self) {
        self.show_flags = 0;
    }

    /// `SetFlags` (GAME 0x47a070, `g_indicators.h:458`).
    pub fn set_flags(&mut self, flags: u32) {
        self.show_flags = flags;
    }

    /// `AddFlags` (GAME 0x47a080, `g_indicators.h:462`).
    pub fn add_flags(&mut self, flags: u32) {
        self.show_flags |= flags;
    }

    /// `RemoveFlags` (GAME 0x47a0a0, `g_indicators.h:466`).
    pub fn remove_flags(&mut self, flags: u32) {
        self.show_flags &= !flags;
    }

    /// `IsShown` (GAME 0x47a0c0, `g_indicators.h:470`).
    pub fn is_shown(&self, flags: u32) -> bool {
        self.show_flags & flags != 0
    }

    /// `PushFlags` (GAME 0x47a0e0, `g_indicators.h:477-481`): pushes the
    /// current flags (member 0x40a8) as `GM_Menu::ExecuteMenu` does while a
    /// menu runs, and returns them.
    pub fn push_flags(&mut self) -> u32 {
        self.show_flags_stack.push(self.show_flags);
        self.show_flags
    }

    /// `PopFlags` (GAME 0x47a2a0, `g_indicators.h:484-491`).
    pub fn pop_flags(&mut self) -> u32 {
        if let Some(flags) = self.show_flags_stack.pop() {
            self.show_flags = flags;
        }
        self.show_flags
    }

    /// `SetSymbol` (GAME 0x47a300, `g_indicators.h:494-501`): `alpha` is
    /// 0..=1; out-of-range host saturation is documented in the module note
    /// (native x86 conversion differs there, inside no tested domain).
    pub fn set_symbol(&mut self, symbol: u32, alpha: f32) {
        self.symbol_index = symbol;
        let mut a = (alpha * 255.0) as u32;
        if a > 0xff {
            a = 0xff;
        }
        self.symbol_color = (a << 24 | 0xffffff) as i32;
    }

    /// `SetProgress` (GAME 0x47a340, `g_indicators.h:503-506`).
    pub fn set_progress(&mut self, value: f32) {
        self.progress = value;
    }

    /// `GetLanguage` (GAME 0x5f70d0, `g_indicators.cpp:249-252`): the two
    /// primary-language letters with the first in the low byte. Staged via
    /// `set_language_from_table`; 0 selects the non-localized menu fallback.
    pub fn get_language(&self) -> u32 {
        self.language
    }

    pub fn set_language_from_table(&mut self, letters: [u8; 4]) {
        self.language = u32::from_le_bytes(letters);
    }

    /// `TimerSetTime` (GAME 0x5f7500, `g_indicators.cpp:405-413`): stores the
    /// time and invalidates the hands (`-1`, which no time has) so they are
    /// computed again by the deferred draw.
    pub fn timer_set_time(&mut self, hour: u32, minute: u32, second: u32) {
        self.timer_hour = hour;
        self.timer_minute = minute;
        self.timer_second = second;
        self.timer_hands_invalid = true;
    }

    /// `TimerSetInterval` (GAME 0x5f7540, `g_indicators.cpp:417-427`):
    /// `interval` seconds from the current clock; the minute hand wraps past
    /// a whole turn. Integer division order matches the native body.
    pub fn timer_set_interval(&mut self, interval: u32) {
        self.timer_interval_ms = interval.wrapping_mul(1000);
        self.timer_interval_minute =
            (self.timer_minute.wrapping_add(interval / 60)) as f32 * (1.0 / 60.0);
        if self.timer_interval_minute > 1.0 {
            self.timer_interval_minute -= 1.0;
        }
        self.timer_interval_second =
            (self.timer_second.wrapping_add(interval % 60)) as f32 * (1.0 / 60.0);
        self.timer_hands_invalid = true;
    }

    /// `TimerGetInterval` (GAME 0x5f7620, `g_indicators.cpp:430-433`).
    pub fn timer_get_interval(&self) -> u32 {
        self.timer_interval_ms / 1000
    }

    /// `TachometrSetGear` (GAME 0x5f8690, `g_indicators.cpp:951-955`): the
    /// gear's column is 11 texels apart; the flag picks the manual/automatic
    /// icon half (`C_Vehicle::auto_gearing`, `CTRL_GEARING`).
    pub fn tachometr_set_gear(&mut self, gear: i32, gearing_mode: bool) {
        self.gear_column = gear as f32 * 11.0;
        self.gearing_mode = gearing_mode;
    }

    /// `TachometrSetValues` (GAME 0x5f86b0, `g_indicators.cpp:964-1023`):
    /// speed arrives in m/s and shows in km/h; needles recompute only while
    /// their value changes. The odometer digits and the `_Vtx` writes stay
    /// with the device owner, so the odometer stages no value here.
    pub fn tachometr_set_values(&mut self, speed_mps: f32, revs: f32, fuel: f32, _odometer_m: f32) {
        let speed = speed_mps * 3.6;
        if speed != self.tach_speed_value {
            self.tach_speed_value = speed;
            self.tach_speed_corners = compute_needle_corners(
                &TACHOMETER_DEF_SPEED,
                speed,
                self.tachometer_pos,
                self.hud_scale,
            );
        }
        if revs != self.tach_revs_value {
            self.tach_revs_value = revs;
            self.tach_revs_corners = compute_needle_corners(
                &TACHOMETER_DEF_REVS,
                revs,
                self.revs_pos,
                self.hud_scale,
            );
        }
        if fuel != self.tach_fuel_value {
            self.tach_fuel_value = fuel;
            self.tach_fuel_corners = compute_needle_corners(
                &TACHOMETER_DEF_FUEL,
                fuel,
                self.tachometer_pos,
                self.hud_scale,
            );
        }
        self.fuel_low = false;
        if fuel < 0.02 {
            self.fuel_low = true;
            if revs < 0.01 {
                self.fuel_light_phase = 0.0;
            }
        }
    }

    /// `PlayerSetLives` (GAME 0x5f88e0, `g_indicators.cpp:1026-1031`):
    /// `"%d"` formats the stored word as signed, matching the native call.
    pub fn player_set_lives(&mut self, lives: u32) {
        self.lives = lives;
        self.lives_text = format!("{}", lives as i32);
    }

    /// `PlayerSetAmmo` (GAME 0x5f8910, `g_indicators.cpp:1034-1040`).
    pub fn player_set_ammo(&mut self, ammo: u32, spare: u32) {
        self.ammo = ammo;
        self.ammo_spare = spare;
        self.ammo_text = format!("{}/{}", ammo as i32, spare as i32);
    }

    /// `PlayerSetAccuracy` (GAME 0x5f8940, `g_indicators.cpp:1043-1046`).
    pub fn player_set_accuracy(&mut self, accuracy: f32) {
        self.accuracy = accuracy;
    }

    /// `PlayerSetWingmanName` (GAME 0x5f8950, `g_indicators.cpp:1049-1053`):
    /// wingman above `MAX_WINGMEN - 1` is ignored, the name masked with 7.
    pub fn player_set_wingman_name(&mut self, wingman: u32, name: u32) {
        if (wingman as usize) <= MAX_WINGMEN - 1 {
            self.wingman_names[wingman as usize] = name & 7;
        }
    }

    /// `PlayerSetWingmanLives` (GAME 0x5f8970, `g_indicators.cpp:1056-1060`).
    pub fn player_set_wingman_lives(&mut self, wingman: u32, lives: u32) {
        if (wingman as usize) <= MAX_WINGMEN - 1 {
            self.wingman_lives[wingman as usize] = lives;
        }
    }

    /// `C_game::Init` player-HUD step (`game/mission/game.cpp:937-947`,
    /// `C_game::Init` PC 0x5a0810): flags set to exactly `INDF_PLAYER`, lives
    /// from the clamped energy fraction, accuracy reset. The energy
    /// clamp-against-max lives with the human owner; the caller stages the
    /// already-clamped-above fraction, `None` when there is no player. The
    /// `(int)` truncation is exact for finite in-range values (see the module
    /// float note); `UpdateControls`/racing/garage work stays deferred.
    pub fn apply_player_hud(&mut self, energy_fraction: Option<f32>) {
        self.set_flags(INDF_PLAYER);
        match energy_fraction {
            Some(fraction) => {
                let clamped = if fraction > 1.0 { 1.0 } else { fraction };
                self.player_set_lives((clamped * 100.0) as i32 as u32);
            }
            None => self.player_set_lives(100),
        }
        self.player_set_accuracy(1.0);
    }

    /// `SubtitlesClear` (GAME 0x5f8990, `g_indicators.cpp:1063-1066`).
    /// Marker scope retains no subtitle payload (text pointers live with the
    /// deferred text owner), so the clear is trivially complete.
    pub fn clear_subtitles(&mut self) {}

    /// `IntroSubtitlesClear` (GAME 0x5f9940, `g_indicators.cpp:1081-1085`),
    /// same retention note as `clear_subtitles`.
    pub fn clear_intro_subtitle_slots(&mut self) {}

    /// `MapEnable` (GAME 0x5f9d10, `g_indicators.cpp:1106-1110`): the
    /// `textDatabase.GetText(0x5f)` refresh stays with the text owner.
    pub fn map_enable(&mut self, enable: bool) {
        self.map_enabled = enable;
    }

    /// `ConsoleClear` (GAME 0x5f9d40, `g_indicators.cpp:1114-1117`): only the
    /// count clears; retained lines stay. `ConsoleAddText` (GAME 0x5f9d50)
    /// and `ConsoleAddTextByID` are stubs/deferred (text owner).
    pub fn console_clear(&mut self) {
        self.console_count = 0;
    }

    /// `CompassSetDestination` (GAME 0x5fa020, `g_indicators.cpp:1126-1129`):
    /// the `I3D_frame *` stays with the scene owner; only presence stages.
    pub fn set_compass_destination(&mut self, present: bool) {
        self.compass_destination_present = present;
    }

    /// `WideScreenSetRatio` (GAME 0x5fa050, `g_indicators.cpp:1135-1203`):
    /// the `_VtxWideScreen` writes stay with the device owner; the ratio
    /// stages. `WideScreenSetOn` (GAME 0x5fa300) is a stub and stays
    /// deferred.
    pub fn set_widescreen_ratio(&mut self, ratio: f32) {
        self.widescreen_ratio = ratio;
    }

    /// `FadeInOutIsDark` (GAME 0x5fa400, `g_indicators.cpp:1225-1228`).
    /// The `FadeInOut` state machine (GAME 0x5fa370/0x5fa3e0) is stubbed
    /// upstream and stays deferred.
    pub fn fade_in_out_is_dark(&self) -> bool {
        self.is_shown(INDF_FADE)
    }

    /// `SetOverlayFade` (GAME 0x5fa410, `g_indicators.cpp:1231-1235`).
    pub fn set_overlay_fade(&mut self, fade: f32, color: u32) {
        self.overlay_fade = fade;
        self.overlay_color = color;
    }

    /// `SetOverlayTexture` pure subset (GAME 0x5fa430,
    /// `g_indicators.cpp:1238-1251`): pointer-identity early-out (host
    /// compares staged bytes), release tracking, opaque flag. The
    /// `igraph->CreateITexture` load stays with the device owner. Cutscene
    /// `Close` passes `NULL` (`C_recorder.cpp:552`).
    pub fn set_overlay_texture(&mut self, name: Option<&[u8]>, opaque: bool, reload: bool) {
        let same = match (&self.overlay_name, name) {
            (None, None) => true,
            (Some(current), Some(next)) => current.as_slice() == next,
            _ => false,
        };
        if same && !reload {
            return;
        }
        self.overlay_texture_requested = false;
        self.overlay_name = name.map(|bytes| bytes.to_vec());
        self.overlay_opaque = opaque;
        if let Some(_staged) = &self.overlay_name {
            self.overlay_texture_requested = true;
        } else {
            self.overlay_opaque = false;
        }
    }

    /// `RadarClear` (GAME 0x5fa4a0, `g_indicators.cpp:1254-1258`). Road edges
    /// are untouched (native clears them only in `GameDone`).
    pub fn radar_clear(&mut self) {
        self.radar_cars.clear();
        self.radar_items.clear();
    }

    /// `RadarRemoveCar` (GAME 0x5fa720, `g_indicators.cpp:1267-1277`):
    /// removes the first matching row. `RadarAddCar` (GAME 0x5fa510) and
    /// `RadarSetPlayerCar` (GAME 0x5fa500) are stubs and stay deferred.
    pub fn radar_remove_car(&mut self, car: u32) -> bool {
        if let Some(index) = self.radar_cars.iter().position(|row| row.car == car) {
            self.radar_cars.remove(index);
            return true;
        }
        false
    }

    /// Stages one radar road edge for the future web owner
    /// (`RadarSetLeftEdge` GAME 0x5fa9f0 / `RadarSetRightEdge` GAME 0x5fabf0,
    /// `g_indicators.cpp:1283/1329`). Native keeps `edge->size() + 1` points
    /// and drops edges below two nodes; vertex allocation stays deferred.
    pub fn stage_radar_edge(&mut self, side: RadarEdgeSide, points: Vec<[f32; 3]>) {
        let kept = if points.len() >= 2 { points } else { Vec::new() };
        match side {
            RadarEdgeSide::Left => self.radar_left_edge = kept,
            RadarEdgeSide::Right => self.radar_right_edge = kept,
        }
    }

    /// `RaceSetStartFlag` (GAME 0x5fadf0, `g_indicators.cpp:1376-1402`).
    /// `RACE_START_DONE`/`RACE_START_NONE` hide the lights; other values
    /// rotate the flag history.
    pub fn race_set_start_flag(&mut self, flag: u32) {
        match flag {
            0 => {
                self.show_flags |= INDF_RACE_START;
                self.race_start_flag_last = 0xff;
                self.race_start_flag = 0;
                self.race_start_last_time = 1.0;
                self.race_start_time = 1.0;
            }
            RACE_START_DONE | RACE_START_NONE => {
                self.show_flags &= !INDF_RACE_START;
                self.race_start_flag_last = 0xff;
                self.race_start_last_time = 1.0;
                self.race_start_flag = 0xff;
                self.race_start_time = 1.0;
            }
            _ => {
                self.show_flags |= INDF_RACE_START;
                self.race_start_flag_last = self.race_start_flag;
                self.race_start_last_time = self.race_start_time;
                self.race_start_flag = flag;
                self.race_start_time = 1.0;
            }
        }
    }

    /// `BonusSetCount` pure subset (GAME 0x5fb010, `g_indicators.cpp:1405-1412`):
    /// the count stores; the `textDatabase.GetText(0xb3)` formatting stays
    /// with the text owner and stages via `stage_bonus_text`.
    pub fn bonus_set_count(&mut self, count: i32) {
        self.bonus_count = count;
        self.bonus_text = None;
    }

    pub fn stage_bonus_text(&mut self, formatted: &str) {
        self.bonus_text = Some(formatted.to_string());
    }

    /// `G_Camera::Begin` CP_SNIPER arm (`game/camera/g_camera.cpp:1213-1215`):
    /// the HUD swaps crosshair for the sniper scope. Wired from
    /// `crate::game_camera_begin::BeginLink::Sniper`.
    pub fn apply_camera_begin_sniper(&mut self) {
        self.show_flags &= !INDF_CROSSHAIR;
        self.show_flags |= INDF_SNIPER;
    }

    /// `G_Camera::End` CP_SNIPER arm (`game/camera/g_camera.cpp:1238-1240`):
    /// restores the crosshair. Wired from
    /// `crate::game_camera_begin::EndApplied::sniper_hud_restore`.
    pub fn apply_camera_end_sniper(&mut self) {
        self.show_flags &= !INDF_SNIPER;
        self.show_flags |= INDF_CROSSHAIR;
    }

    /// `C_recorder::Close` HUD subset (GAME 0x59c6a0,
    /// `game/mission/C_recorder.cpp:552-553,588-590`): overlay fade/color and
    /// texture cleared, viewport-center flag cleared, subtitles cleared,
    /// `INDF_CUTSCENE` removed. Texture `Release` calls, `SetWBuffer`, the
    /// actor/placement restore and `DabingReset` stay with their owners.
    pub fn apply_recorder_close(&mut self) {
        self.set_overlay_texture(None, false, false);
        self.set_overlay_fade(0.0, 0);
        self.parhelia_center = false;
        self.clear_subtitles();
        self.show_flags &= !INDF_CUTSCENE;
    }

    /// `ParheliaSetCenter` flag half (GAME 0x604a80,
    /// `g_indicators.cpp:4064-4108`): the clears/presents/viewport/camera
    /// work stays with the scene/device owners.
    pub fn set_parhelia_center(&mut self, center: bool) {
        self.parhelia_center = center;
    }

    /// `ResolutionChanged` pure layout (GAME 0x601520,
    /// `g_indicators.cpp:3512-3596`) over staged screen dimensions: Parhelia
    /// detection, edges, scales, dial/clock positions, screen center, and
    /// the six needle-corner recomputes. All `_Vtx*` writes stay with the
    /// device owner. `Scrn_sx()/Scrn_sy()` integer division for the triple
    /// edges is matched with 32-bit wrapping arithmetic.
    pub fn resolution_changed(&mut self, screen_sx: u32, screen_sy: u32) {
        let sx = screen_sx as f32;
        let sy = screen_sy as f32;
        let mut layout = ParheliaLayout::None;
        if sx > sy * 2.0 {
            layout = ParheliaLayout::Dual;
        }
        if sx > sy * 3.0 {
            layout = ParheliaLayout::Triple;
        }
        self.parhelia_layout = layout;
        let (left, right) = match layout {
            ParheliaLayout::Triple => {
                let third = screen_sx / 3;
                let two_thirds = screen_sx.wrapping_mul(2) / 3;
                ([third as f32, 0.0], [two_thirds as f32, sy])
            }
            _ => ([0.0, 0.0], [sx, sy]),
        };
        self.screen_edges = [left, right];
        self.screen_size = [sx, sy];
        self.hud_size = [sx, sy];
        let scale = match layout {
            ParheliaLayout::Dual => [sx * (1.0 / 1600.0), sy * (1.0 / 600.0)],
            ParheliaLayout::Triple => [sx * (1.0 / 2400.0), sy * (1.0 / 600.0)],
            ParheliaLayout::None => [sx * (1.0 / 800.0), sy * (1.0 / 600.0)],
        };
        self.screen_scale = scale;
        self.hud_scale = scale;
        if layout == ParheliaLayout::Dual {
            self.hud_size[0] *= 0.5;
        } else if layout == ParheliaLayout::Triple {
            self.hud_size[0] *= 1.0 / 3.0;
        }
        self.tachometer_pos = [
            right[0] - scale[0] * 189.0,
            right[1] - scale[1] * 189.0,
        ];
        self.revs_pos = [
            right[0] - 259.0 * scale[0] + scale[0] * 11.0,
            right[1] - 84.0 * scale[1],
        ];
        self.timer_pos = [-106.0 * scale[0] + right[0], 20.0 * scale[1]];
        self.screen_center = [sx / 2.0, sy / 2.0];
        // Native recomputes every needle here (`g_indicators.cpp:3578-3588`);
        // timer hands hold the invalid -1 value, which clamps to the scale
        // start exactly as the native `t` clamp does.
        self.tach_speed_corners = compute_needle_corners(
            &TACHOMETER_DEF_SPEED,
            self.tach_speed_value,
            self.tachometer_pos,
            scale,
        );
        self.tach_revs_corners = compute_needle_corners(
            &TACHOMETER_DEF_REVS,
            self.tach_revs_value,
            self.revs_pos,
            scale,
        );
        self.tach_fuel_corners = compute_needle_corners(
            &TACHOMETER_DEF_FUEL,
            self.tach_fuel_value,
            self.tachometer_pos,
            scale,
        );
        // Timer hands recompute from the invalid value; the per-hand defs
        // match the native call order (hours, minutes, seconds).
        self.timer_hand_corners = [
            compute_needle_corners(&TIMER_DEF_HOURS, -1.0, self.timer_pos, scale),
            compute_needle_corners(&TIMER_DEF_MINUTES, -1.0, self.timer_pos, scale),
            compute_needle_corners(&TIMER_DEF_SECONDS, -1.0, self.timer_pos, scale),
        ];
    }

    // Typed accessors for the wired surface.
    pub fn show_flags(&self) -> u32 {
        self.show_flags
    }
    pub fn show_flags_stack_depth(&self) -> usize {
        self.show_flags_stack.len()
    }
    pub fn is_created(&self) -> bool {
        self.created
    }
    pub fn hud_textures_present(&self) -> bool {
        self.hud_textures_present
    }
    pub fn dial_asset(&self) -> Option<&'static str> {
        self.dial_asset
    }
    pub fn icons_asset(&self) -> Option<&'static str> {
        self.icons_asset
    }
    pub fn map_texture_present(&self) -> bool {
        self.map_texture_present
    }
    pub fn timer_hms(&self) -> (u32, u32, u32) {
        (self.timer_hour, self.timer_minute, self.timer_second)
    }
    pub fn timer_interval_ms(&self) -> u32 {
        self.timer_interval_ms
    }
    pub fn timer_interval_fractions(&self) -> (f32, f32) {
        (self.timer_interval_minute, self.timer_interval_second)
    }
    pub fn timer_hands_invalid(&self) -> bool {
        self.timer_hands_invalid
    }
    pub fn tachometer_values(&self) -> (f32, f32, f32) {
        (self.tach_speed_value, self.tach_revs_value, self.tach_fuel_value)
    }
    pub fn gear_column(&self) -> f32 {
        self.gear_column
    }
    pub fn gearing_mode(&self) -> bool {
        self.gearing_mode
    }
    pub fn fuel_low(&self) -> bool {
        self.fuel_low
    }
    pub fn fuel_light_phase(&self) -> f32 {
        self.fuel_light_phase
    }
    pub fn lives(&self) -> u32 {
        self.lives
    }
    pub fn lives_text(&self) -> &str {
        &self.lives_text
    }
    pub fn ammo(&self) -> (u32, u32) {
        (self.ammo, self.ammo_spare)
    }
    pub fn ammo_text(&self) -> &str {
        &self.ammo_text
    }
    pub fn accuracy(&self) -> f32 {
        self.accuracy
    }
    pub fn wingman_name(&self, wingman: usize) -> Option<u32> {
        self.wingman_names.get(wingman).copied()
    }
    pub fn wingman_lives(&self, wingman: usize) -> Option<u32> {
        self.wingman_lives.get(wingman).copied()
    }
    pub fn compass_destination_present(&self) -> bool {
        self.compass_destination_present
    }
    pub fn console_count(&self) -> u32 {
        self.console_count
    }
    pub fn console_first(&self) -> u32 {
        self.console_first
    }
    pub fn thief_indicator(&self) -> f32 {
        self.thief_indicator
    }
    pub fn overlay(&self) -> (f32, u32, Option<&[u8]>, bool, bool) {
        (
            self.overlay_fade,
            self.overlay_color,
            self.overlay_name.as_deref(),
            self.overlay_texture_requested,
            self.overlay_opaque,
        )
    }
    pub fn radar_car_count(&self) -> usize {
        self.radar_cars.len()
    }
    pub fn radar_item_count(&self) -> usize {
        self.radar_items.len()
    }
    pub fn radar_player_car(&self) -> Option<u32> {
        self.radar_player_car
    }
    pub fn radar_scale(&self) -> f32 {
        self.radar_scale
    }
    pub fn radar_edges(&self) -> (&[[f32; 3]], &[[f32; 3]]) {
        (&self.radar_left_edge, &self.radar_right_edge)
    }
    pub fn race_start(&self) -> (u32, u32, f32, f32) {
        (
            self.race_start_flag_last,
            self.race_start_flag,
            self.race_start_last_time,
            self.race_start_time,
        )
    }
    pub fn bonus_count(&self) -> i32 {
        self.bonus_count
    }
    pub fn bonus_text(&self) -> Option<&str> {
        self.bonus_text.as_deref()
    }
    pub fn symbol(&self) -> (u32, i32) {
        (self.symbol_index, self.symbol_color)
    }
    pub fn progress(&self) -> f32 {
        self.progress
    }
    pub fn map_enabled(&self) -> bool {
        self.map_enabled
    }
    pub fn parhelia_center(&self) -> bool {
        self.parhelia_center
    }
    pub fn parhelia_layout(&self) -> ParheliaLayout {
        self.parhelia_layout
    }
    pub fn layout(&self) -> ([f32; 2], [f32; 2], [f32; 2], [f32; 2]) {
        (
            self.screen_size,
            self.screen_scale,
            self.hud_size,
            self.hud_scale,
        )
    }
    pub fn screen_edges(&self) -> [[f32; 2]; 2] {
        self.screen_edges
    }
    pub fn dial_positions(&self) -> ([f32; 2], [f32; 2], [f32; 2], [f32; 2]) {
        (
            self.tachometer_pos,
            self.revs_pos,
            self.timer_pos,
            self.screen_center,
        )
    }
    pub fn widescreen_ratio(&self) -> f32 {
        self.widescreen_ratio
    }
}

/// Non-HUD capability requested from the indicator-scoped load/save adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndicatorsEffectError {
    Unavailable(&'static str),
}

impl std::fmt::Display for IndicatorsEffectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndicatorsEffectError::Unavailable(what) => {
                write!(f, "indicators load/save adapter cannot provide {what}")
            }
        }
    }
}

impl std::error::Error for IndicatorsEffectError {}

/// Real `LoadSaveEffects` backend for the three HUD-owned calls
/// (`G_LoadSaveClass::Init` GAME 0x604ee0 at `g_loadsave.cpp:116-120`,
/// `Close` GAME 0x605030 at `g_loadsave.cpp:143`). Every other capability
/// (profile defaults/tables, controls, enumeration, timer nulling, menu
/// mission, profile file, text and car-database teardown) reports its
/// unavailable owner instead of mocking success.
pub struct IndicatorsLoadSave<'a> {
    state: &'a mut IndicatorsState,
    display: Option<(u32, u32)>,
    clock: TimerClock,
}

impl<'a> IndicatorsLoadSave<'a> {
    pub fn new(state: &'a mut IndicatorsState, clock: TimerClock) -> Self {
        Self {
            state,
            display: None,
            clock,
        }
    }

    pub fn with_display(mut self, screen_sx: u32, screen_sy: u32) -> Self {
        self.display = Some((screen_sx, screen_sy));
        self
    }
}

impl LoadSaveEffects for IndicatorsLoadSave<'_> {
    type Error = IndicatorsEffectError;

    fn all_default(&mut self, _profile: &mut super::game_loadsave_lifecycle::ProfileState) -> Result<(), Self::Error> {
        Err(IndicatorsEffectError::Unavailable("profile defaults"))
    }

    fn create_indicators(&mut self) -> Result<(), Self::Error> {
        self.state.create(self.clock);
        Ok(())
    }

    fn resolution_changed(&mut self) -> Result<(), Self::Error> {
        match self.display {
            Some((sx, sy)) => {
                self.state.resolution_changed(sx, sy);
                Ok(())
            }
            None => Err(IndicatorsEffectError::Unavailable("display dimensions")),
        }
    }

    fn clear_indicator_slot_40a4(&mut self) -> Result<(), Self::Error> {
        self.state.clear_flags_slot();
        Ok(())
    }

    fn enumerate_profiles(
        &mut self,
        _profile: &mut super::game_loadsave_lifecycle::ProfileState,
    ) -> Result<(), Self::Error> {
        Err(IndicatorsEffectError::Unavailable("profile enumeration"))
    }

    fn null_timer(&mut self) -> Result<(), Self::Error> {
        Err(IndicatorsEffectError::Unavailable("timer nulling"))
    }

    fn open_menu_mission(&mut self) -> Result<(), Self::Error> {
        Err(IndicatorsEffectError::Unavailable("menu mission"))
    }

    fn execute_profile_select(&mut self) -> Result<i32, Self::Error> {
        Err(IndicatorsEffectError::Unavailable("profile selection"))
    }

    fn profile_load(
        &mut self,
        _profile: &mut super::game_loadsave_lifecycle::ProfileState,
    ) -> Result<(), Self::Error> {
        Err(IndicatorsEffectError::Unavailable("profile load"))
    }

    fn profile_save(
        &mut self,
        _profile: &super::game_loadsave_lifecycle::ProfileState,
    ) -> Result<(), Self::Error> {
        Err(IndicatorsEffectError::Unavailable("profile save"))
    }

    fn destroy_indicators(&mut self) -> Result<(), Self::Error> {
        self.state.destroy();
        Ok(())
    }

    fn unload_text(&mut self) -> Result<(), Self::Error> {
        Err(IndicatorsEffectError::Unavailable("text unload"))
    }

    fn close_car_database(&mut self) -> Result<(), Self::Error> {
        Err(IndicatorsEffectError::Unavailable("car database"))
    }
}

/// `C_game::Init` HUD-step failure over the indicator adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndicatorsInitError {
    pub step: InitStep,
    pub detail: &'static str,
}

impl std::fmt::Display for IndicatorsInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.step, self.detail)
    }
}

impl std::error::Error for IndicatorsInitError {}

/// `InitOperations` over the live HUD owner for exactly the two
/// indicator-owned steps: `IndicatorsGameInit` runs the `GameInit` texture
/// plan (`game/mission/game.cpp:334`), `PlayerHud` runs the clamp/accuracy
/// body (`game/mission/game.cpp:937-947`). All other steps report the step
/// they refuse, preserving native stop-at-first-unowned-step order.
pub struct IndicatorsInit<'a> {
    pub state: &'a mut IndicatorsState,
    pub night_mission: bool,
    pub speedometer_mph: bool,
    pub player_energy_fraction: Option<f32>,
}

impl InitOperations for IndicatorsInit<'_> {
    type Error = IndicatorsInitError;

    fn execute(&mut self, step: InitStep) -> Result<(), Self::Error> {
        match step {
            InitStep::IndicatorsGameInit => {
                let plan = IndicatorsState::game_init_texture_plan(
                    self.night_mission,
                    self.speedometer_mph,
                );
                self.state.apply_game_init_texture_plan(plan);
                Ok(())
            }
            InitStep::PlayerHud => {
                self.state.apply_player_hud(self.player_energy_fraction);
                Ok(())
            }
            _ => Err(IndicatorsInitError {
                step,
                detail: "not an indicator-owned init step",
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn staged_clock() -> TimerClock {
        TimerClock {
            hour: 13,
            minute: 30,
            second: 45,
        }
    }

    fn approx(left: f32, right: f32) -> bool {
        (left - right).abs() <= 1e-3
    }

    #[test]
    fn flag_inline_semantics_match_header() {
        let mut state = IndicatorsState::constructed(staged_clock());
        assert_eq!(state.show_flags(), 0);
        state.set_flags(INDF_PLAYER | INDF_TIMER);
        assert!(state.is_shown(INDF_PLAYER));
        assert!(!state.is_shown(INDF_CROSSHAIR));
        state.add_flags(INDF_CROSSHAIR);
        assert!(state.is_shown(INDF_CROSSHAIR));
        state.remove_flags(INDF_TIMER);
        assert!(!state.is_shown(INDF_TIMER));
        // Push/pop nest exactly like the vector at 0x40a8.
        assert_eq!(state.push_flags(), INDF_PLAYER | INDF_CROSSHAIR);
        state.set_flags(INDF_MENU);
        assert_eq!(state.pop_flags(), INDF_PLAYER | INDF_CROSSHAIR);
        assert_eq!(state.show_flags_stack_depth(), 0);
        // Empty pop keeps the flags (native size check).
        assert_eq!(state.pop_flags(), INDF_PLAYER | INDF_CROSSHAIR);
        // Load/save slot clear is a direct zero store.
        state.add_flags(INDF_FADE);
        state.clear_flags_slot();
        assert_eq!(state.show_flags(), 0);
    }

    #[test]
    fn reset_values_matches_native_order_and_defaults() {
        let state = IndicatorsState::constructed(staged_clock());
        assert_eq!(state.show_flags(), 0);
        assert_eq!(state.show_flags_stack_depth(), 0);
        assert_eq!(state.timer_hms(), (13, 30, 45));
        assert!(state.timer_hands_invalid());
        assert_eq!(state.timer_interval_ms(), 130_000);
        assert_eq!(state.timer_get_interval(), 130);
        let (minute, second) = state.timer_interval_fractions();
        assert!(approx(minute, 32.0 / 60.0));
        assert!(approx(second, 55.0 / 60.0));
        assert_eq!(state.gear_column(), 0.0);
        assert!(!state.gearing_mode());
        assert_eq!(state.tachometer_values(), (0.0, 0.0, 0.0));
        // Native fuel 0 with revs 0 ends low with phase 0.
        assert!(state.fuel_low());
        assert_eq!(state.fuel_light_phase(), 0.0);
        assert_eq!(state.lives(), 100);
        assert_eq!(state.lives_text(), "100");
        assert_eq!(state.ammo(), (16, 30));
        assert_eq!(state.ammo_text(), "16/30");
        assert_eq!(state.accuracy(), 1.0);
        for wingman in 0..MAX_WINGMEN {
            assert_eq!(state.wingman_name(wingman), Some(0));
            assert_eq!(state.wingman_lives(wingman), Some(100));
        }
        assert!(!state.compass_destination_present());
        assert_eq!(state.thief_indicator(), 0.5);
        assert_eq!((state.console_first(), state.console_count()), (0, 0));
        assert_eq!(state.radar_player_car(), None);
        assert_eq!(state.radar_car_count(), 0);
        assert_eq!(state.radar_item_count(), 0);
        assert_eq!(state.radar_scale(), 0.9);
        assert_eq!(state.symbol(), (0, -1));
        assert!(!state.map_enabled());
        assert!(!state.parhelia_center());
        assert_eq!(state.bonus_count(), BONUS_NONE);
        assert_eq!(state.bonus_text(), None);
        let (fade, color, name, requested, opaque) = state.overlay();
        assert_eq!((fade, color, requested, opaque), (0.0, 0, false, false));
        assert_eq!(name, None);
        // Top bits of the flag word stay clear.
        assert_eq!(state.show_flags() & 0xffff_0000, 0);
    }

    #[test]
    fn create_destroy_lifecycle_tracks_device_presence() {
        let mut state = IndicatorsState::constructed(staged_clock());
        assert!(!state.is_created());
        state.create(staged_clock());
        assert!(state.is_created());
        assert!(state.hud_textures_present());
        assert!(!state.map_texture_present());
        assert_eq!(state.lives(), 100);
        state.destroy();
        assert!(!state.is_created());
        assert!(!state.hud_textures_present());
        assert_eq!(state.dial_asset(), None);
    }

    #[test]
    fn game_init_plan_selects_native_asset_names() {
        assert_eq!(
            IndicatorsState::game_init_texture_plan(false, false),
            GameInitTexturePlan {
                dial_asset: "2int.tga",
                icons_asset: "2intauto.tga",
            }
        );
        assert_eq!(
            IndicatorsState::game_init_texture_plan(false, true).dial_asset,
            "2intmph.tga"
        );
        assert_eq!(
            IndicatorsState::game_init_texture_plan(true, false),
            GameInitTexturePlan {
                dial_asset: "2intnoc.tga",
                icons_asset: "2intauton.tga",
            }
        );
        assert_eq!(
            IndicatorsState::game_init_texture_plan(true, true).dial_asset,
            "2intnocmph.tga"
        );
        let mut state = IndicatorsState::constructed(staged_clock());
        let plan = IndicatorsState::game_init_texture_plan(true, true);
        state.apply_game_init_texture_plan(plan);
        assert_eq!(state.dial_asset(), Some("2intnocmph.tga"));
        assert_eq!(state.icons_asset(), Some("2intauton.tga"));
    }

    #[test]
    fn game_done_clears_map_edges_bonus_and_interval() {
        let mut state = IndicatorsState::constructed(staged_clock());
        state.map_texture_present = true;
        state.stage_radar_edge(RadarEdgeSide::Left, vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);
        state.stage_bonus_text("staged");
        state.game_done();
        assert!(!state.map_texture_present());
        assert!(state.radar_edges().0.is_empty() && state.radar_edges().1.is_empty());
        assert_eq!(state.bonus_count(), BONUS_NONE);
        assert_eq!(state.bonus_text(), None);
        assert_eq!(state.timer_get_interval(), 0);
        // The hiding case stores 0xff, not the passed flag
        // (g_indicators.cpp:1387-1394: RACE_START_DONE/NONE arm assigns
        // race_start_flag = 0xff).
        assert_eq!(state.race_start().1, 0xff);
        assert!(!state.is_shown(INDF_RACE_START));
        // Dial textures survive GameDone on the PC path.
        state.apply_game_init_texture_plan(IndicatorsState::game_init_texture_plan(false, false));
        state.game_done();
        assert_eq!(state.dial_asset(), Some("2int.tga"));
    }

    #[test]
    fn player_hud_matches_game_cpp_clamp_body() {
        let mut state = IndicatorsState::constructed(staged_clock());
        state.set_flags(INDF_TIMER | INDF_RADAR);
        state.apply_player_hud(Some(0.755));
        // SetFlags(4): exact store, not a union.
        assert_eq!(state.show_flags(), INDF_PLAYER);
        // (int)(0.755 * 100) truncates to 75.
        assert_eq!(state.lives(), 75);
        assert_eq!(state.lives_text(), "75");
        assert_eq!(state.accuracy(), 1.0);
        // Above-max clamps to 100 like the native property clamp.
        state.apply_player_hud(Some(1.2));
        assert_eq!(state.lives(), 100);
        // No player stages exactly 100.
        state.apply_player_hud(None);
        assert_eq!(state.lives(), 100);
        assert_eq!(state.show_flags(), INDF_PLAYER);
    }

    #[test]
    fn tachometer_values_convert_and_flag_fuel() {
        let mut state = IndicatorsState::constructed(staged_clock());
        state.resolution_changed(800, 600);
        // 10 m/s shows 36 km/h up to host f32 rounding of the 3.6 factor.
        state.tachometr_set_values(10.0, 0.5, 0.5, 1234.0);
        let (speed, revs, fuel) = state.tachometer_values();
        assert!(approx(speed, 36.0));
        assert_eq!((revs, fuel), (0.5, 0.5));
        assert!(!state.fuel_low());
        state.tachometr_set_gear(3, true);
        assert_eq!(state.gear_column(), 33.0);
        assert!(state.gearing_mode());
        // Below 2% fuel with stopped revs zeroes the light phase.
        state.tachometr_set_values(0.0, 0.005, 0.015, 0.0);
        assert!(state.fuel_low());
        assert_eq!(state.fuel_light_phase(), 0.0);
    }

    #[test]
    fn wingman_guards_reject_out_of_range() {
        let mut state = IndicatorsState::constructed(staged_clock());
        state.player_set_wingman_name(2, 0xf);
        state.player_set_wingman_lives(3, 42);
        assert_eq!(state.wingman_name(2), Some(0x7));
        assert_eq!(state.wingman_lives(3), Some(42));
        state.player_set_wingman_name(4, 1);
        state.player_set_wingman_lives(9, 1);
        assert_eq!(state.wingman_name(4), None);
        assert_eq!(state.wingman_lives(9), None);
        assert_eq!(state.wingman_name(0), Some(0));
    }

    #[test]
    fn lives_ammo_text_uses_signed_format() {
        let mut state = IndicatorsState::constructed(staged_clock());
        state.player_set_lives(7);
        state.player_set_ammo(3, 12);
        assert_eq!(state.lives_text(), "7");
        assert_eq!(state.ammo_text(), "3/12");
    }

    #[test]
    fn race_start_flag_rotates_history() {
        let mut state = IndicatorsState::constructed(staged_clock());
        state.race_set_start_flag(0);
        assert!(state.is_shown(INDF_RACE_START));
        assert_eq!(state.race_start(), (0xff, 0, 1.0, 1.0));
        state.race_set_start_flag(2);
        assert_eq!(state.race_start(), (0, 2, 1.0, 1.0));
        state.race_set_start_flag(RACE_START_NONE);
        assert!(!state.is_shown(INDF_RACE_START));
        assert_eq!(state.race_start().1, 0xff);
        state.race_set_start_flag(RACE_START_DONE);
        assert!(!state.is_shown(INDF_RACE_START));
    }

    #[test]
    fn sniper_swap_matches_camera_arms() {
        let mut state = IndicatorsState::constructed(staged_clock());
        state.set_flags(INDF_CROSSHAIR | INDF_PLAYER);
        state.apply_camera_begin_sniper();
        assert!(!state.is_shown(INDF_CROSSHAIR));
        assert!(state.is_shown(INDF_SNIPER));
        assert!(state.is_shown(INDF_PLAYER));
        state.apply_camera_end_sniper();
        assert!(state.is_shown(INDF_CROSSHAIR));
        assert!(!state.is_shown(INDF_SNIPER));
    }

    #[test]
    fn recorder_close_clears_hud_subset() {
        let mut state = IndicatorsState::constructed(staged_clock());
        state.set_flags(INDF_CUTSCENE | INDF_PLAYER);
        state.set_overlay_fade(0.5, 0x112233);
        state.set_overlay_texture(Some(b"shot.tga"), true, false);
        state.set_parhelia_center(true);
        state.apply_recorder_close();
        let (fade, color, name, requested, opaque) = state.overlay();
        assert_eq!((fade, color, requested, opaque), (0.0, 0, false, false));
        assert_eq!(name, None);
        assert!(!state.parhelia_center());
        assert!(!state.is_shown(INDF_CUTSCENE));
        assert!(state.is_shown(INDF_PLAYER));
    }

    #[test]
    fn overlay_texture_identity_matches_pointer_semantics() {
        let mut state = IndicatorsState::constructed(staged_clock());
        state.set_overlay_texture(Some(b"shot.tga"), true, false);
        assert_eq!(state.overlay().3, true);
        // Same pointer without reload keeps the texture (native early-out).
        state.set_overlay_texture(Some(b"shot.tga"), false, false);
        assert_eq!(state.overlay().3, true);
        assert!(state.overlay().4);
        // Reload forces the release path.
        state.set_overlay_texture(Some(b"shot.tga"), false, true);
        assert_eq!(state.overlay().3, true);
        assert!(!state.overlay().4);
        // NULL clears and forces opaque off.
        state.set_overlay_texture(None, true, false);
        assert_eq!(state.overlay(), (0.0, 0, None, false, false));
    }

    #[test]
    fn symbol_alpha_follows_native_conversion() {
        let mut state = IndicatorsState::constructed(staged_clock());
        state.set_symbol(2, 0.5);
        // (uint)(0.5 * 255) truncates to 127.
        assert_eq!(state.symbol(), (2, 0x7fffffffu32 as i32));
        state.set_symbol(1, 2.0);
        assert_eq!(state.symbol().1, 0xffffffffu32 as i32);
        state.set_symbol(3, 0.0);
        assert_eq!(state.symbol(), (3, 0x00ffffff));
        state.set_progress(0.25);
        assert_eq!(state.progress(), 0.25);
    }

    #[test]
    fn fade_dark_reads_flag_word() {
        let mut state = IndicatorsState::constructed(staged_clock());
        assert!(!state.fade_in_out_is_dark());
        state.add_flags(INDF_FADE);
        assert!(state.fade_in_out_is_dark());
    }

    #[test]
    fn radar_remove_matches_first_row_only() {
        let mut state = IndicatorsState::constructed(staged_clock());
        state.radar_cars.push(RadarCar { car: 7, color: 1 });
        state.radar_cars.push(RadarCar { car: 7, color: 2 });
        assert!(state.radar_remove_car(7));
        assert_eq!(state.radar_car_count(), 1);
        assert!(!state.radar_remove_car(9));
        // RadarClear keeps staged road edges (native only clears cars/items).
        state.stage_radar_edge(RadarEdgeSide::Right, vec![[0.0; 3], [1.0; 3]]);
        state.radar_clear();
        assert_eq!(state.radar_car_count(), 0);
        assert_eq!(state.radar_edges().1.len(), 2);
        // Below two nodes stages nothing, like the native count check.
        state.stage_radar_edge(RadarEdgeSide::Left, vec![[0.0; 3]]);
        assert!(state.radar_edges().0.is_empty());
    }

    #[test]
    fn resolution_changed_layout_matches_native_branches() {
        let mut state = IndicatorsState::constructed(staged_clock());
        state.resolution_changed(800, 600);
        assert_eq!(state.parhelia_layout(), ParheliaLayout::None);
        let (size, scale, hud, _) = state.layout();
        for (got, want) in [size, scale, hud]
            .iter()
            .zip([[800.0, 600.0], [1.0, 1.0], [800.0, 600.0]].iter())
        {
            assert!(approx(got[0], want[0]) && approx(got[1], want[1]));
        }
        let (tacho, revs, timer, center) = state.dial_positions();
        for (got, want) in [tacho, revs, timer, center].iter().zip(
            [
                [611.0, 411.0],
                [552.0, 516.0],
                [694.0, 20.0],
                [400.0, 300.0],
            ]
            .iter(),
        ) {
            assert!(approx(got[0], want[0]) && approx(got[1], want[1]));
        }
        // Dual: wider than 2:1, HUD on the halved screen.
        state.resolution_changed(1700, 600);
        assert_eq!(state.parhelia_layout(), ParheliaLayout::Dual);
        let (_, scale, hud, _) = state.layout();
        assert!(approx(scale[0], 1700.0 / 1600.0) && approx(scale[1], 1.0));
        assert!(approx(hud[0], 850.0));
        // Triple: wider than 3:1, integer-division middle-screen edges.
        state.resolution_changed(2500, 600);
        assert_eq!(state.parhelia_layout(), ParheliaLayout::Triple);
        assert_eq!(state.screen_edges(), [[833.0, 0.0], [1666.0, 600.0]]);
        let (_, scale, hud, _) = state.layout();
        assert!(approx(scale[0], 2500.0 / 2400.0));
        assert!(approx(hud[0], 2500.0 / 3.0));
    }

    #[test]
    fn needle_corners_match_f64_reference_and_clamp() {
        // 60 km/h on the 240 scale: t = 0.25, angle = 283.5 degrees.
        let got = compute_needle_corners(&TACHOMETER_DEF_SPEED, 60.0, [0.0, 0.0], [1.0, 1.0]);
        let angle = (283.5f64).to_radians();
        let (s, c) = angle.sin_cos();
        let expect = |along: f64, across: f64| {
            [
                (90.5 + c * across + s * along) as f32,
                (91.5 + s * across - c * along) as f32,
            ]
        };
        for (got, want) in [
            (got.tip_right, expect(80.0, 0.9)),
            (got.tail_right, expect(-10.0, 1.8)),
            (got.tip_left, expect(80.0, -0.9)),
            (got.tail_left, expect(-10.0, -1.8)),
        ] {
            assert!(approx(got[0], want[0]) && approx(got[1], want[1]));
        }
        // Out-of-scale values clamp to the scale ends.
        let hi = compute_needle_corners(&TACHOMETER_DEF_SPEED, 999.0, [0.0, 0.0], [1.0, 1.0]);
        let full = compute_needle_corners(&TACHOMETER_DEF_SPEED, 240.0, [0.0, 0.0], [1.0, 1.0]);
        assert_eq!(hi, full);
        let lo = compute_needle_corners(&TACHOMETER_DEF_SPEED, -5.0, [0.0, 0.0], [1.0, 1.0]);
        let zero = compute_needle_corners(&TACHOMETER_DEF_SPEED, 0.0, [0.0, 0.0], [1.0, 1.0]);
        assert_eq!(lo, zero);
        // Vertex order and texture coordinates match the native writes.
        let verts = fill_needle_vertices(&got, 0xff74130d);
        assert_eq!((verts[4], verts[5]), (verts[2], verts[1]));
        assert_eq!(
            (verts[0].u, verts[0].v, verts[1].u, verts[2].v, verts[3].u),
            (0.0, 0.0, 1.0, 1.0, 1.0)
        );
        assert!(verts.iter().all(|v| v.z == 0.0 && v.rhw == 1.0 && v.color == 0xff74130d));
    }

    #[test]
    fn rect_and_diamond_vertices_match_native_writes() {
        let mut rect = [TlVertex::colored_point(0.0, 0.0, 0, 0.0, 0.0); 6];
        set_rect_vertices([10.0, 20.0], [30.0, 40.0], 0.1, 0.2, 0.3, 0.4, 7, &mut rect);
        assert_eq!((rect[0].x, rect[0].y), (10.0, 20.0));
        assert_eq!((rect[1].x, rect[3].x, rect[3].y), (40.0, 40.0, 60.0));
        assert_eq!((rect[4], rect[5]), (rect[2], rect[1]));
        let mut diamond = [TlVertex::colored_point(0.0, 0.0, 0, 0.0, 0.0); 6];
        set_diamond_vertices([10.0, 20.0], [3.0, 4.0], 0.1, 0.2, 0.3, 0.4, 9, &mut diamond);
        assert_eq!((diamond[0].x, diamond[0].y), (7.0, 20.0));
        assert_eq!((diamond[1].x, diamond[1].y), (10.0, 16.0));
        assert_eq!((diamond[2].x, diamond[2].y), (10.0, 24.0));
        assert_eq!((diamond[3].x, diamond[3].y), (13.0, 20.0));
        assert_eq!((diamond[4], diamond[5]), (diamond[2], diamond[1]));
    }

    #[test]
    fn language_stages_get_language_convention() {
        let mut state = IndicatorsState::constructed(staged_clock());
        assert_eq!(state.get_language(), 0);
        state.set_language_from_table(*b"en\0\0");
        assert_eq!(state.get_language(), 0x6e65);
    }

    #[test]
    fn loadsave_adapter_drives_hud_calls_and_refuses_others() {
        use crate::game_loadsave_lifecycle::{LoadSaveEffects, ProfileState};
        use crate::game_loadsave_controls::ProfileBindings;
        use crate::game_controls::ControlBinding;
        let mut state = IndicatorsState::constructed(staged_clock());
        state.set_flags(INDF_PLAYER);
        let empty = ControlBinding {
            half: 0,
            device: 0,
            code: 0,
            joystick_index: 0,
        };
        let mut profile = ProfileState {
            init_flag: false,
            race_cars: 0,
            unknown_1f0: 0,
            unknown_9c: 0,
            unknown_1e8: 0,
            camera_setting: 0,
            bindings: ProfileBindings::from_rows(
                [empty; crate::game_loadsave_controls::PROFILE_CONTROL_COUNT],
            ),
            profiles: Vec::new(),
        };
        let clock = staged_clock();
        let mut adapter = IndicatorsLoadSave::new(&mut state, clock).with_display(800, 600);
        adapter.create_indicators().unwrap();
        adapter.resolution_changed().unwrap();
        assert!(adapter.state.is_created());
        adapter.clear_indicator_slot_40a4().unwrap();
        assert_eq!(adapter.state.show_flags(), 0);
        assert!(adapter.all_default(&mut profile).is_err());
        assert!(adapter.null_timer().is_err());
        assert!(adapter.profile_save(&profile).is_err());
        adapter.destroy_indicators().unwrap();
        assert!(!adapter.state.is_created());
        // Without staged dimensions the resolution step refuses.
        let mut bare = IndicatorsLoadSave::new(adapter.state, clock);
        assert!(bare.resolution_changed().is_err());
    }

    #[test]
    fn init_adapter_runs_hud_steps_in_native_order() {
        let mut state = IndicatorsState::constructed(staged_clock());
        let mut ops = IndicatorsInit {
            state: &mut state,
            night_mission: true,
            speedometer_mph: false,
            player_energy_fraction: Some(0.5),
        };
        ops.execute(InitStep::IndicatorsGameInit).unwrap();
        assert_eq!(ops.state.dial_asset(), Some("2intnoc.tga"));
        ops.execute(InitStep::PlayerHud).unwrap();
        assert_eq!(ops.state.show_flags(), INDF_PLAYER);
        assert_eq!(ops.state.lives(), 50);
        let error = ops.execute(InitStep::PoliceBeginShift).unwrap_err();
        assert_eq!(error.step, InitStep::PoliceBeginShift);
    }

    #[test]
    fn timer_interval_zero_wraps_from_current_clock() {
        let mut state = IndicatorsState::constructed(staged_clock());
        // GameDone calls TimerSetInterval(0): fractions derive from 13:30:45.
        state.game_done();
        let (minute, second) = state.timer_interval_fractions();
        assert!(approx(minute, 30.0 / 60.0));
        assert!(approx(second, 45.0 / 60.0));
        // Past a whole turn the minute hand wraps once.
        state.timer_set_time(0, 59, 0);
        state.timer_set_interval(120);
        let (minute, _) = state.timer_interval_fractions();
        assert!(approx(minute, 1.0 / 60.0));
    }

    #[test]
    fn compass_map_and_parhelia_stage_presence_only() {
        let mut state = IndicatorsState::constructed(staged_clock());
        state.set_compass_destination(true);
        state.map_enable(true);
        state.set_parhelia_center(true);
        assert!(state.compass_destination_present());
        assert!(state.map_enabled());
        assert!(state.parhelia_center());
        state.set_compass_destination(false);
        assert!(!state.compass_destination_present());
    }
}
