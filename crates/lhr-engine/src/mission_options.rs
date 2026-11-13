//! Mission video/audio option state (`S_mission_options`), marker-only scope.
//!
//! Game.exe SHA-256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27,
//! base 0x400000; addresses below are GAME RVAs. Reference: mafia-re 113b5b6
//! `game/mission/Mission.h:23-63` (struct, member offsets 0x0-0x18, inline
//! ctor and getters) with bodies at `game/mission/Mission.cpp:1770-1842`,
//! and the option enums at `common/settings.h:21-37`.
//!
//! Owned here: the plain `Copy` option state the mission lifecycle will hold
//! (wiring that owner is deferred and untouched here) plus the clamping
//! setters the Audio & Video menu drives (`main` hands each `setup.exe` value
//! through a setter and saves them back on quit, `Mission.h:20-22`):
//! `SetLevelOfDetail` GAME 0x545b20 (`Mission.cpp:1771-1778`),
//! `SetDrawDistance` GAME 0x545b60 (`Mission.cpp:1781-1788`),
//! `SetGameEffects` GAME 0x545ba0 (`Mission.cpp:1791-1798`),
//! `SetGameSounds` GAME 0x545bd0 (`Mission.cpp:1801-1808`),
//! `SetShadows` GAME 0x545c00 (`Mission.cpp:1811-1820`),
//! `GetShadows` GAME 0x545c60 (`Mission.cpp:1823-1832`),
//! `SetParticles` GAME 0x545cb0 (`Mission.cpp:1835-1842`).
//! The inline `GetLevelOfDetail`/`GetDrawDistance` reads
//! (`Mission.h:53-55`) and `GetParticles` (GAME 0x47ae30, `Mission.h:56-62`)
//! are plain member reads; members stay public as in the original struct.
//!
//! DEFERRED with explicit markers (no credit): holding this state in
//! `C_mission` (`options` at 0x90, `Mission.h:154`) and every call site
//! (`main` setup handoff, menu `GetTextID`/`SetTextID` paths in
//! `game/ui/g_m_func.cpp:437-567`, the `RS_LOD_QUALITY` / camera-range
//! application in `ApplySettings`/`RestoreSettings`, `Mission.cpp:1750-1766`).

#![forbid(unsafe_code)]

/// "In Game Effects" / "In Game Sounds" / particles: Low
/// (`common/settings.h:21-25`).
pub const OPTION_LOW: i32 = 0;
/// Medium (`common/settings.h:21-25`).
pub const OPTION_MEDIUM: i32 = 1;
/// High (`common/settings.h:21-25`).
pub const OPTION_HIGH: i32 = 2;

/// Shadow menu choice: Very Low (`common/settings.h:29-37`).
pub const SHADOWS_VERY_LOW: i32 = 0;
/// Shadow menu choice: Low (`common/settings.h:29-37`).
pub const SHADOWS_LOW: i32 = 1;
/// Shadow menu choice: Medium, also the `GetShadows` unknown-level fallback
/// (`common/settings.h:29-37`, `game/mission/Mission.cpp:1831`).
pub const SHADOWS_MEDIUM: i32 = 2;
/// Shadow menu choice: High (`common/settings.h:29-37`).
pub const SHADOWS_HIGH: i32 = 3;
/// Shadow menu choice: Very High (`common/settings.h:29-37`).
pub const SHADOWS_VERY_HIGH: i32 = 4;
/// Shadow menu choice: Highest (`common/settings.h:29-37`).
pub const SHADOWS_HIGHEST: i32 = 5;
/// Shadow menu choice count (`common/settings.h:29-37`).
pub const NUM_SHADOWS: usize = 6;
/// Shadow level per menu choice, Very Low to Highest
/// (`game/mission/Mission.cpp:1813,1825`).
pub const SHADOW_LEVELS: [i32; NUM_SHADOWS] = [0, 1, 3, 6, 9, 15];

/// `level_of_detail` lower bound (`game/mission/Mission.cpp:1774`).
pub const LEVEL_OF_DETAIL_MIN: f32 = 0.4;
/// `level_of_detail` upper bound (`game/mission/Mission.cpp:1776`).
pub const LEVEL_OF_DETAIL_MAX: f32 = 2.0;
/// `draw_distance` lower bound (`game/mission/Mission.cpp:1784`).
pub const DRAW_DISTANCE_MIN: f32 = 0.3;
/// `draw_distance` upper bound (`game/mission/Mission.cpp:1786`).
pub const DRAW_DISTANCE_MAX: f32 = 1.0;

/// Video/audio options (mafia-re `game/mission/Mission.h:23-32`). Plain
/// `Copy` state; the mission lifecycle owner holds it (deferred, untouched).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MissionOptions {
    /// 0x0, 0.4 to 2: the engine's `RS_LOD_QUALITY`
    /// (mafia-re `game/mission/Mission.h:25`).
    pub level_of_detail: f32,
    /// 0x4, 0.3 to 1: the part of `far_range` the camera sees
    /// (mafia-re `game/mission/Mission.h:26`).
    pub draw_distance: f32,
    /// 0x8, the scene's camera range, which `Open` stores
    /// (mafia-re `game/mission/Mission.h:27`).
    pub far_range: f32,
    /// 0xc, "In Game Effects": Low, Medium or High
    /// (mafia-re `game/mission/Mission.h:28`).
    pub game_effects: i32,
    /// 0x10, "In Game Sounds": Low, Medium or High
    /// (mafia-re `game/mission/Mission.h:29`).
    pub game_sounds: i32,
    /// 0x14, one of `SHADOW_LEVELS` for the menu's Very Low to Highest
    /// (mafia-re `game/mission/Mission.h:30`).
    pub shadows: i32,
    /// 0x18, Low, Medium or High (mafia-re `game/mission/Mission.h:31`).
    pub particles: i32,
}

impl MissionOptions {
    /// Inline ctor defaults (mafia-re `game/mission/Mission.h:34-42`):
    /// effects/sounds/particles High, shadows level 3. `far_range` has no
    /// initializer in the original (VC6 constant stores group by value);
    /// `Open` stores the camera range there, so the port seeds 0.0.
    pub fn new() -> Self {
        Self {
            level_of_detail: 1.0,
            draw_distance: 1.0,
            far_range: 0.0,
            game_effects: OPTION_HIGH,
            game_sounds: OPTION_HIGH,
            // Native inline ctor stores the raw value 3 (a translated
            // level, Medium), not a translated index (Mission.h:34-42).
            shadows: 3,
            particles: OPTION_HIGH,
        }
    }

    /// Clamp 0.4..=2.0 (GAME 0x545b20, mafia-re
    /// `game/mission/Mission.cpp:1770-1778`): store first, then the lower
    /// bound branch returns at once and the upper bound compares the
    /// parameter (kept as the `if`/`else if` shape).
    // Mirrors the native store-then-branch shape documented above.
    #[allow(clippy::manual_clamp)]
    pub fn set_level_of_detail(&mut self, level: f32) {
        self.level_of_detail = level;
        if level < LEVEL_OF_DETAIL_MIN {
            self.level_of_detail = LEVEL_OF_DETAIL_MIN;
        } else if level > LEVEL_OF_DETAIL_MAX {
            self.level_of_detail = LEVEL_OF_DETAIL_MAX;
        }
    }

    /// Clamp 0.3..=1.0 (GAME 0x545b60, mafia-re
    /// `game/mission/Mission.cpp:1780-1788`): two separate bounds checks
    /// reading the member back.
    // Mirrors the native member re-read shape documented above.
    #[allow(clippy::manual_clamp)]
    pub fn set_draw_distance(&mut self, distance: f32) {
        self.draw_distance = distance;
        if self.draw_distance < DRAW_DISTANCE_MIN {
            self.draw_distance = DRAW_DISTANCE_MIN;
        }
        if self.draw_distance > DRAW_DISTANCE_MAX {
            self.draw_distance = DRAW_DISTANCE_MAX;
        }
    }

    /// Clamp `OPTION_LOW..=OPTION_HIGH` (GAME 0x545ba0, mafia-re
    /// `game/mission/Mission.cpp:1790-1798`).
    pub fn set_game_effects(&mut self, level: i32) {
        self.game_effects = level.clamp(OPTION_LOW, OPTION_HIGH);
    }

    /// Clamp `OPTION_LOW..=OPTION_HIGH`, same shape as `SetGameEffects`
    /// (GAME 0x545bd0, mafia-re `game/mission/Mission.cpp:1800-1808`).
    pub fn set_game_sounds(&mut self, level: i32) {
        self.game_sounds = level.clamp(OPTION_LOW, OPTION_HIGH);
    }

    /// Store the shadow level for menu index 0..=5 (GAME 0x545c00, mafia-re
    /// `game/mission/Mission.cpp:1810-1820`): clamp the index, then map
    /// through `{0, 1, 3, 6, 9, 15}`.
    pub fn set_shadows(&mut self, index: i32) {
        self.shadows = index.clamp(SHADOWS_VERY_LOW, SHADOWS_HIGHEST);
        self.shadows = SHADOW_LEVELS[self.shadows as usize];
    }

    /// Menu index of the stored shadow level, `SHADOWS_MEDIUM` for an unknown
    /// level (GAME 0x545c60, mafia-re `game/mission/Mission.cpp:1822-1832`).
    pub fn get_shadows(&self) -> i32 {
        for (index, level) in SHADOW_LEVELS.iter().enumerate() {
            if self.shadows == *level {
                return index as i32;
            }
        }
        SHADOWS_MEDIUM
    }

    /// Clamp `OPTION_LOW..=OPTION_HIGH` (GAME 0x545cb0, mafia-re
    /// `game/mission/Mission.cpp:1834-1842`).
    pub fn set_particles(&mut self, level: i32) {
        self.particles = level.clamp(OPTION_LOW, OPTION_HIGH);
    }

    /// FUNCTION: GAME RVA 0x7ae30, VA 0x47ae30 (mafia-re
    /// `game/mission/Mission.h:58-62`, `S_mission_options::GetParticles`):
    /// reads the stored particles level.
    pub fn get_particles(&self) -> i32 {
        self.particles
    }

    /// Working name (no outline RVA; inline read-only, mafia-re
    /// `game/mission/Mission.h:27`, `S_mission_options::far_range` at +0x8):
    /// reads the camera range `Open` stores. The native body is inlined at
    /// every use, so no Game.exe function entry is claimed.
    pub fn get_far_range(&self) -> f32 {
        self.far_range
    }

    /// Working name (no outline RVA; inline store, mafia-re
    /// `game/mission/Mission.cpp:891,897,940` at pin 113b5b6,
    /// `C_mission::OpenBin`): stages the camera range `Open` keeps in
    /// `far_range` (+0x8). `OpenBin` writes the parsed value (250.0 or the
    /// file's own) with no dedicated setter, so no Game.exe function entry
    /// is claimed.
    pub fn set_far_range(&mut self, range: f32) {
        self.far_range = range;
    }

    /// Working name (no outline RVA; inline read-only, mafia-re
    /// `game/mission/Mission.h:28`, `S_mission_options::game_effects` at
    /// +0xc): reads the stored effects level. The native body is inlined
    /// at every use, so no Game.exe function entry is claimed.
    pub fn get_game_effects(&self) -> i32 {
        self.game_effects
    }

    /// Working name (no outline RVA; inline read-only, mafia-re
    /// `game/mission/Mission.h:29`, `S_mission_options::game_sounds` at
    /// +0x10): reads the stored sounds level. The native body is inlined
    /// at every use, so no Game.exe function entry is claimed.
    pub fn get_game_sounds(&self) -> i32 {
        self.game_sounds
    }

    /// Working name (no outline RVA; inline read-only, mafia-re
    /// `game/mission/Mission.h:54`, `S_mission_options::GetLevelOfDetail`):
    /// reads the stored level of detail. The native body is inlined at every
    /// call site, so no Game.exe function entry is claimed.
    pub fn get_level_of_detail(&self) -> f32 {
        self.level_of_detail
    }

    /// Working name (no outline RVA; inline read-only, mafia-re
    /// `game/mission/Mission.h:55`, `S_mission_options::GetDrawDistance`):
    /// reads the stored draw distance. The native body is inlined at every
    /// call site, so no Game.exe function entry is claimed.
    pub fn get_draw_distance(&self) -> f32 {
        self.draw_distance
    }

    /// Video-option application (GAME 0x545a30, mafia-re
    /// `game/mission/Mission.cpp:1738-1754`): with an open city cache the
    /// camera sees `far_range * draw_distance`, otherwise `far_range`; the
    /// level-of-detail clamp below 1.0 applies only with a closed cache. Only
    /// option reads happen here; camera/driver/weather effects run through
    /// the host backends, which own no I3D/camera/weather/device state.
    pub fn update_settings<C, D, W>(
        &self,
        base_cache_open: bool,
        camera: Option<&mut C>,
        driver: &mut D,
        weather: &mut W,
    ) where
        C: MissionSettingsCamera,
        D: MissionSettingsDriver,
        W: MissionSettingsWeather,
    {
        if let Some(camera) = camera {
            if base_cache_open {
                camera.set_range(0.05, self.far_range * self.draw_distance);
            } else {
                camera.set_range(0.05, self.far_range);
            }
            camera.update();
        }
        let mut lod = self.level_of_detail;
        if !base_cache_open && lod < 1.0 {
            lod = 1.0;
        }
        driver.set_lod_quality_bits(lod.to_bits() as i32);
        weather.refresh_weather(false, u32::MAX);
    }

    /// Scene-local restore (GAME 0x545ad0, mafia-re
    /// `game/mission/Mission.cpp:1756-1766`): camera back to `far_range` and
    /// `RS_LOD_QUALITY` back to 1.0. Same host-callback split as
    /// `update_settings`; no base-cache read and no weather call.
    pub fn restore_settings<C, D>(&self, camera: Option<&mut C>, driver: &mut D)
    where
        C: MissionSettingsCamera,
        D: MissionSettingsDriver,
    {
        if let Some(camera) = camera {
            camera.set_range(0.05, self.far_range);
            camera.update();
        }
        driver.set_lod_quality_bits(1.0f32.to_bits() as i32);
    }
}

/// Host camera effects for `update_settings`/`restore_settings` (marker-only
/// contract; the scene/camera owner stays deferred). `set_range` mirrors
/// `I3D_camera::SetRange` and `update` mirrors `I3D_camera::Update`.
pub trait MissionSettingsCamera {
    fn set_range(&mut self, near: f32, far: f32);
    fn update(&mut self);
}

/// Host driver effect for the `RS_LOD_QUALITY` store. The bit pattern is the
/// original `*(int *)&lod` / `I3DFloatAsInt` word, not a rounded integer.
pub trait MissionSettingsDriver {
    fn set_lod_quality_bits(&mut self, bits: i32);
}

/// Host weather effect for `game->OsefujPocasi(false, (uint)-1)` (mafia-re
/// `game/mission/Mission.cpp:1753`). Arguments keep the native order and the
/// all-bits-set second word.
pub trait MissionSettingsWeather {
    fn refresh_weather(&mut self, enable: bool, param: u32);
}

impl Default for MissionOptions {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctor_defaults_match_inline_ctor() {
        let options = MissionOptions::new();
        assert_eq!(options.level_of_detail, 1.0);
        assert_eq!(options.draw_distance, 1.0);
        assert_eq!(options.game_effects, OPTION_HIGH);
        assert_eq!(options.game_sounds, OPTION_HIGH);
        assert_eq!(options.shadows, 3);
        assert_eq!(options.particles, OPTION_HIGH);
        // Raw default 3 scans to index 2 (Mission.cpp:1823-1831).
        assert_eq!(options.get_shadows(), SHADOWS_MEDIUM);
    }

    #[test]
    fn level_of_detail_clamps_to_0_4_and_2_0() {
        let mut options = MissionOptions::new();
        options.set_level_of_detail(0.39);
        assert_eq!(options.level_of_detail, LEVEL_OF_DETAIL_MIN);
        options.set_level_of_detail(2.01);
        assert_eq!(options.level_of_detail, LEVEL_OF_DETAIL_MAX);
        options.set_level_of_detail(0.4);
        assert_eq!(options.level_of_detail, 0.4);
        options.set_level_of_detail(2.0);
        assert_eq!(options.level_of_detail, 2.0);
        options.set_level_of_detail(1.25);
        assert_eq!(options.level_of_detail, 1.25);
    }

    #[test]
    fn draw_distance_clamps_to_0_3_and_1_0() {
        let mut options = MissionOptions::new();
        options.set_draw_distance(0.29);
        assert_eq!(options.draw_distance, DRAW_DISTANCE_MIN);
        options.set_draw_distance(1.01);
        assert_eq!(options.draw_distance, DRAW_DISTANCE_MAX);
        options.set_draw_distance(0.3);
        assert_eq!(options.draw_distance, 0.3);
        options.set_draw_distance(1.0);
        assert_eq!(options.draw_distance, 1.0);
        options.set_draw_distance(0.7);
        assert_eq!(options.draw_distance, 0.7);
    }

    #[test]
    fn game_effects_clamps_low_to_high() {
        let mut options = MissionOptions::new();
        options.set_game_effects(OPTION_LOW - 1);
        assert_eq!(options.game_effects, OPTION_LOW);
        options.set_game_effects(OPTION_HIGH + 1);
        assert_eq!(options.game_effects, OPTION_HIGH);
        options.set_game_effects(OPTION_MEDIUM);
        assert_eq!(options.game_effects, OPTION_MEDIUM);
    }

    #[test]
    fn game_sounds_clamps_low_to_high() {
        let mut options = MissionOptions::new();
        options.set_game_sounds(OPTION_LOW - 1);
        assert_eq!(options.game_sounds, OPTION_LOW);
        options.set_game_sounds(OPTION_HIGH + 1);
        assert_eq!(options.game_sounds, OPTION_HIGH);
        options.set_game_sounds(OPTION_MEDIUM);
        assert_eq!(options.game_sounds, OPTION_MEDIUM);
    }

    #[test]
    fn shadows_maps_index_through_level_table() {
        let mut options = MissionOptions::new();
        for (index, level) in SHADOW_LEVELS.iter().enumerate() {
            options.set_shadows(index as i32);
            assert_eq!(
                options.shadows, *level,
                "menu index {index} must map to level {level}"
            );
        }
    }

    #[test]
    fn shadows_clamps_index_bounds() {
        let mut options = MissionOptions::new();
        options.set_shadows(SHADOWS_VERY_LOW - 5);
        assert_eq!(options.shadows, SHADOW_LEVELS[0]);
        options.set_shadows(SHADOWS_HIGHEST + 99);
        assert_eq!(options.shadows, SHADOW_LEVELS[NUM_SHADOWS - 1]);
    }

    #[test]
    fn get_shadows_round_trips_all_indices() {
        let mut options = MissionOptions::new();
        for index in SHADOWS_VERY_LOW..=SHADOWS_HIGHEST {
            options.set_shadows(index);
            assert_eq!(options.get_shadows(), index);
        }
    }

    #[test]
    fn get_shadows_unknown_level_yields_medium() {
        let mut options = MissionOptions::new();
        options.shadows = 2;
        assert_eq!(options.get_shadows(), SHADOWS_MEDIUM);
        options.shadows = 7;
        assert_eq!(options.get_shadows(), SHADOWS_MEDIUM);
        options.shadows = -1;
        assert_eq!(options.get_shadows(), SHADOWS_MEDIUM);
    }

    #[test]
    fn particles_clamps_low_to_high() {
        let mut options = MissionOptions::new();
        options.set_particles(OPTION_LOW - 1);
        assert_eq!(options.particles, OPTION_LOW);
        options.set_particles(OPTION_HIGH + 1);
        assert_eq!(options.particles, OPTION_HIGH);
        options.set_particles(OPTION_MEDIUM);
        assert_eq!(options.particles, OPTION_MEDIUM);
    }

    struct FakeCamera {
        ranges: Vec<[f32; 2]>,
        updates: usize,
    }
    struct FakeDriver {
        lod_bits: Vec<i32>,
    }
    struct FakeWeather {
        calls: Vec<(bool, u32)>,
    }
    impl MissionSettingsCamera for FakeCamera {
        fn set_range(&mut self, near: f32, far: f32) {
            self.ranges.push([near, far]);
        }
        fn update(&mut self) {
            self.updates += 1;
        }
    }
    impl MissionSettingsDriver for FakeDriver {
        fn set_lod_quality_bits(&mut self, bits: i32) {
            self.lod_bits.push(bits);
        }
    }
    impl MissionSettingsWeather for FakeWeather {
        fn refresh_weather(&mut self, enable: bool, param: u32) {
            self.calls.push((enable, param));
        }
    }

    #[test]
    fn update_settings_open_cache_scales_range_and_keeps_low_lod() {
        let options = MissionOptions {
            level_of_detail: 0.4,
            draw_distance: 0.5,
            far_range: 250.0,
            ..MissionOptions::new()
        };
        let mut camera = FakeCamera {
            ranges: Vec::new(),
            updates: 0,
        };
        let mut driver = FakeDriver {
            lod_bits: Vec::new(),
        };
        let mut weather = FakeWeather { calls: Vec::new() };
        options.update_settings(true, Some(&mut camera), &mut driver, &mut weather);
        assert_eq!(camera.ranges, vec![[0.05, 125.0]]);
        assert_eq!(camera.updates, 1);
        assert_eq!(driver.lod_bits, vec![0.4f32.to_bits() as i32]);
        assert_eq!(weather.calls, vec![(false, u32::MAX)]);
    }

    #[test]
    fn update_settings_closed_cache_clamps_lod_below_one() {
        let options = MissionOptions {
            level_of_detail: 0.4,
            draw_distance: 0.5,
            far_range: 250.0,
            ..MissionOptions::new()
        };
        let mut camera = FakeCamera {
            ranges: Vec::new(),
            updates: 0,
        };
        let mut driver = FakeDriver {
            lod_bits: Vec::new(),
        };
        let mut weather = FakeWeather { calls: Vec::new() };
        options.update_settings(false, Some(&mut camera), &mut driver, &mut weather);
        assert_eq!(camera.ranges, vec![[0.05, 250.0]]);
        assert_eq!(camera.updates, 1);
        assert_eq!(driver.lod_bits, vec![1.0f32.to_bits() as i32]);
        assert_eq!(weather.calls, vec![(false, u32::MAX)]);
    }

    #[test]
    fn update_settings_without_camera_still_drives_lod_and_weather() {
        let options = MissionOptions {
            level_of_detail: 1.5,
            ..MissionOptions::new()
        };
        let mut driver = FakeDriver {
            lod_bits: Vec::new(),
        };
        let mut weather = FakeWeather { calls: Vec::new() };
        let camera: Option<&mut FakeCamera> = None;
        options.update_settings(false, camera, &mut driver, &mut weather);
        assert_eq!(driver.lod_bits, vec![1.5f32.to_bits() as i32]);
        assert_eq!(weather.calls, vec![(false, u32::MAX)]);
    }

    #[test]
    fn restore_settings_returns_range_and_lod_quality() {
        let options = MissionOptions {
            far_range: 250.0,
            level_of_detail: 0.4,
            ..MissionOptions::new()
        };
        let mut camera = FakeCamera {
            ranges: Vec::new(),
            updates: 0,
        };
        let mut driver = FakeDriver {
            lod_bits: Vec::new(),
        };
        options.restore_settings(Some(&mut camera), &mut driver);
        assert_eq!(camera.ranges, vec![[0.05, 250.0]]);
        assert_eq!(camera.updates, 1);
        assert_eq!(driver.lod_bits, vec![1.0f32.to_bits() as i32]);
    }

    #[test]
    fn get_particles_rva_7ae30_reads_stored_level() {
        // S_mission_options::GetParticles GAME RVA 0x7ae30 (VA 0x47ae30).
        let options = MissionOptions::new();
        assert_eq!(options.get_particles(), OPTION_HIGH);
        let mut custom = MissionOptions::new();
        custom.set_particles(OPTION_LOW);
        assert_eq!(custom.get_particles(), OPTION_LOW);
    }

    #[test]
    fn get_level_of_detail_working_name_reads_stored_value() {
        // Inline S_mission_options::GetLevelOfDetail (Mission.h:54); no
        // outline RVA is claimed.
        let mut options = MissionOptions::new();
        assert_eq!(options.get_level_of_detail(), 1.0);
        options.set_level_of_detail(1.5);
        assert_eq!(options.get_level_of_detail(), 1.5);
        options.set_level_of_detail(99.0);
        assert_eq!(options.get_level_of_detail(), LEVEL_OF_DETAIL_MAX);
    }

    #[test]
    fn get_draw_distance_working_name_reads_stored_value() {
        // Inline S_mission_options::GetDrawDistance (Mission.h:55); no
        // outline RVA is claimed.
        let mut options = MissionOptions::new();
        assert_eq!(options.get_draw_distance(), 1.0);
        options.set_draw_distance(0.7);
        assert_eq!(options.get_draw_distance(), 0.7);
        options.set_draw_distance(-1.0);
        assert_eq!(options.get_draw_distance(), DRAW_DISTANCE_MIN);
    }

    #[test]
    fn get_far_range_working_name_reads_stored_range() {
        // Inline S_mission_options::far_range read (Mission.h:27, +0x8); no
        // outline RVA is claimed.
        let mut options = MissionOptions::new();
        assert_eq!(options.get_far_range(), 0.0);
        options.far_range = 250.0;
        assert_eq!(options.get_far_range(), 250.0);
    }

    #[test]
    fn set_far_range_working_name_stages_camera_range() {
        // Working name: OpenBin far_range store (Mission.cpp:891,897,940,
        // +0x8); no outline setter exists, so no RVA is claimed.
        let mut options = MissionOptions::new();
        assert_eq!(options.get_far_range(), 0.0);
        options.set_far_range(250.0);
        assert_eq!(options.get_far_range(), 250.0);
        options.set_far_range(123.5);
        assert_eq!(options.get_far_range(), 123.5);
    }

    #[test]
    fn get_game_effects_working_name_reads_stored_level() {
        // Inline S_mission_options::game_effects read (Mission.h:28, +0xc);
        // no outline RVA is claimed.
        let mut options = MissionOptions::new();
        assert_eq!(options.get_game_effects(), OPTION_HIGH);
        options.set_game_effects(OPTION_LOW);
        assert_eq!(options.get_game_effects(), OPTION_LOW);
    }

    #[test]
    fn get_game_sounds_working_name_reads_stored_level() {
        // Inline S_mission_options::game_sounds read (Mission.h:29, +0x10);
        // no outline RVA is claimed.
        let mut options = MissionOptions::new();
        assert_eq!(options.get_game_sounds(), OPTION_HIGH);
        options.set_game_sounds(OPTION_MEDIUM);
        assert_eq!(options.get_game_sounds(), OPTION_MEDIUM);
    }
}
