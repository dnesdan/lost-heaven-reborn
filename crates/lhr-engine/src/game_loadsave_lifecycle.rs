//! PC G_LoadSaveClass::Init/Close dependency composition over live owners.
//!
//! mafia-re f42a25d, game/ui/g_loadsave.cpp, Game.exe 0x604ee0 and
//! 0x605030. G_LoadSaveClass is a PS2 name mapped to these PC entries. This
//! host adapter requires real HUD, profile, timer, menu and mission effects;
//! it grants no original-function credit until those owners are installed.

use crate::game_controls::{ControlBindingBackend, ControlFailure, GameControls};
use crate::game_definition_tables::{
    DefinitionTableBackend, GameDefinitionTables, TableLoadError, TableUnloadError,
};
use crate::game_loadsave_controls::{apply_controls, ProfileBindings};

/// Fields directly read or written by Init, Close, ApplyControls and Reset.
/// Other profile bytes live in the eventual profile file owner; this structure
/// does not claim a mirrored C++ class layout.
pub struct ProfileState {
    pub init_flag: bool,
    pub race_cars: u32,
    pub unknown_1f0: u32,
    pub unknown_9c: u32,
    pub unknown_1e8: u32,
    pub camera_setting: u32,
    pub bindings: ProfileBindings,
    pub profiles: Vec<[u8; 0x54]>,
}
impl ProfileState {
    fn begin_initialization(&mut self) {
        self.init_flag = true;
        self.race_cars = 0;
        self.unknown_1f0 = 0;
        self.unknown_9c = 0;
        self.unknown_1e8 = 0;
    }
}
impl crate::mission_components::CameraProfile for ProfileState {
    type Error = std::convert::Infallible;
    fn camera_setting(&mut self) -> Result<u32, Self::Error> {
        Ok(self.camera_setting)
    }
}

/// Every call is a required actual-owner capability. The PC source ignores
/// menu-mission Open's native result, but the backend must still perform it.
/// The three HUD calls are owned by `crate::indicators::IndicatorsState`
/// (see `crate::indicators::IndicatorsLoadSave`): `create_indicators` is
/// `G_IndicatorsClass::Create` (GAME 0x5f70e0, `g_indicators.cpp:256`),
/// `clear_indicator_slot_40a4` is the direct `show_flags = 0` store at member
/// 0x40a4 (`G_LoadSaveClass::Init` GAME 0x604ee0, `g_loadsave.cpp:120`), and
/// `destroy_indicators` is `G_IndicatorsClass::Destroy` (GAME 0x5f7250,
/// `g_indicators.cpp:298`).
pub trait LoadSaveEffects {
    type Error;
    fn all_default(&mut self, profile: &mut ProfileState) -> Result<(), Self::Error>;
    /// HUD owner `create`: `IndicatorsState::create` (GAME 0x5f70e0).
    fn create_indicators(&mut self) -> Result<(), Self::Error>;
    fn resolution_changed(&mut self) -> Result<(), Self::Error>;
    /// HUD owner slot clear: `IndicatorsState::clear_flags_slot` (direct
    /// `show_flags` member 0x40a4 zero store, `g_loadsave.cpp:120`).
    fn clear_indicator_slot_40a4(&mut self) -> Result<(), Self::Error>;
    fn enumerate_profiles(&mut self, profile: &mut ProfileState) -> Result<(), Self::Error>;
    fn null_timer(&mut self) -> Result<(), Self::Error>;
    fn open_menu_mission(&mut self) -> Result<(), Self::Error>;
    fn execute_profile_select(&mut self) -> Result<i32, Self::Error>;
    fn profile_load(&mut self, profile: &mut ProfileState) -> Result<(), Self::Error>;
    fn profile_save(&mut self, profile: &ProfileState) -> Result<(), Self::Error>;
    /// HUD owner `destroy`: `IndicatorsState::destroy` (GAME 0x5f7250).
    fn destroy_indicators(&mut self) -> Result<(), Self::Error>;
    fn unload_text(&mut self) -> Result<(), Self::Error>;
    fn close_car_database(&mut self) -> Result<(), Self::Error>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum LoadSaveError<TableE, ControlE, EffectE> {
    Table(TableLoadError<TableE>),
    Control(ControlFailure<ControlE>),
    Effect(EffectE),
    Interrupted,
}
pub type LoadSaveInitResult<TableE, ControlE, EffectE> =
    Result<bool, LoadSaveError<TableE, ControlE, EffectE>>;
#[derive(Debug, PartialEq, Eq)]
pub enum LoadSaveCloseError<E> {
    Effect(E),
    Table(TableUnloadError),
    Interrupted,
}

fn init_effect<T, TableE, ControlE, EffectE>(
    interrupted: &mut bool,
    result: Result<T, EffectE>,
) -> Result<T, LoadSaveError<TableE, ControlE, EffectE>> {
    result.map_err(|error| {
        *interrupted = true;
        LoadSaveError::Effect(error)
    })
}

pub struct GameLoadSave {
    pub profile: ProfileState,
    pub profile_ignore_save: bool,
    init_interrupted: bool,
    close_interrupted: bool,
}
impl GameLoadSave {
    pub fn adopt(profile: ProfileState, profile_ignore_save: bool) -> Self {
        Self {
            profile,
            profile_ignore_save,
            init_interrupted: false,
            close_interrupted: false,
        }
    }
    /// Source order of Init. A missing table is native false from the table
    /// loader but this caller ignores it and continues with the retained
    /// partial tables. Any unavailable backend stops at its exact prefix.
    pub fn init<T, C, E>(
        &mut self,
        tables: &mut GameDefinitionTables,
        controls: &mut GameControls,
        table_backend: &mut T,
        control_backend: &mut C,
        effects: &mut E,
    ) -> LoadSaveInitResult<T::Error, C::Error, E::Error>
    where
        T: DefinitionTableBackend,
        C: ControlBindingBackend,
        E: LoadSaveEffects,
    {
        if self.init_interrupted || self.close_interrupted {
            return Err(LoadSaveError::Interrupted);
        }
        self.profile.begin_initialization();
        init_effect(
            &mut self.init_interrupted,
            effects.all_default(&mut self.profile),
        )?;
        init_effect(&mut self.init_interrupted, effects.create_indicators())?;
        tables.load(table_backend).map_err(|error| {
            self.init_interrupted = true;
            LoadSaveError::Table(error)
        })?;
        apply_controls(controls, &self.profile.bindings, control_backend).map_err(|error| {
            self.init_interrupted = true;
            LoadSaveError::Control(error)
        })?;
        init_effect(&mut self.init_interrupted, effects.resolution_changed())?;
        init_effect(
            &mut self.init_interrupted,
            effects.clear_indicator_slot_40a4(),
        )?;
        init_effect(
            &mut self.init_interrupted,
            effects.enumerate_profiles(&mut self.profile),
        )?;
        init_effect(&mut self.init_interrupted, effects.null_timer())?;
        init_effect(&mut self.init_interrupted, effects.open_menu_mission())?;
        let choice = init_effect(&mut self.init_interrupted, effects.execute_profile_select())?;
        self.profile.profiles.clear();
        if choice == 0xf {
            self.profile_ignore_save = true;
            return Ok(false);
        }
        init_effect(
            &mut self.init_interrupted,
            effects.profile_load(&mut self.profile),
        )?;
        apply_controls(controls, &self.profile.bindings, control_backend).map_err(|error| {
            self.init_interrupted = true;
            LoadSaveError::Control(error)
        })?;
        self.profile_ignore_save = false;
        Ok(true)
    }

    /// Close is admitted after an unfinished Init, as CloseSystem may run
    /// after a partial startup. It saves only if the actual global flag allows,
    /// then destroys indicators, releases source tables, text and car database.
    pub fn close<E>(
        &mut self,
        tables: &mut GameDefinitionTables,
        effects: &mut E,
    ) -> Result<(), LoadSaveCloseError<E::Error>>
    where
        E: LoadSaveEffects,
    {
        if self.close_interrupted {
            return Err(LoadSaveCloseError::Interrupted);
        }
        // A host file operation may have failed after opening a definition
        // file. Native Close has no evidence for cleaning that interrupted
        // host state, so stop before saving, destroying HUD, or dropping its
        // partially filled allocation and live file identity.
        if let Err(error) = tables.ensure_unload_ready() {
            self.close_interrupted = true;
            return Err(LoadSaveCloseError::Table(error));
        }
        if !self.profile_ignore_save {
            if let Err(error) = effects.profile_save(&self.profile) {
                self.close_interrupted = true;
                return Err(LoadSaveCloseError::Effect(error));
            }
        }
        if let Err(error) = effects.destroy_indicators() {
            self.close_interrupted = true;
            return Err(LoadSaveCloseError::Effect(error));
        }
        tables.unload().map_err(|error| {
            self.close_interrupted = true;
            LoadSaveCloseError::Table(error)
        })?;
        if let Err(error) = effects.unload_text() {
            self.close_interrupted = true;
            return Err(LoadSaveCloseError::Effect(error));
        }
        if let Err(error) = effects.close_car_database() {
            self.close_interrupted = true;
            return Err(LoadSaveCloseError::Effect(error));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_controls::{
        ControlAllocationSeed, ControlBinding, ControlGraph, ControlJoystick, ControlRefBackend,
    };
    use crate::game_definition_tables::{DefinitionKind, TableFile};
    use crate::game_loadsave_controls::PROFILE_CONTROL_COUNT;

    struct Tables {
        missing: Option<DefinitionKind>,
        fail_read: Option<DefinitionKind>,
    }
    impl DefinitionTableBackend for Tables {
        type Error = &'static str;
        fn open(&mut self, kind: DefinitionKind) -> Result<Option<TableFile>, Self::Error> {
            Ok((self.missing != Some(kind)).then_some(TableFile(kind as u64 + 1)))
        }
        fn seek_end(&mut self, _: TableFile) -> Result<usize, Self::Error> {
            Ok(0xa4)
        }
        fn seek_begin(&mut self, _: TableFile) -> Result<(), Self::Error> {
            Ok(())
        }
        fn allocate(&mut self, size: usize) -> Result<Vec<u8>, Self::Error> {
            Ok(vec![0xa5; size])
        }
        fn read_into(&mut self, file: TableFile, bytes: &mut [u8]) -> Result<(), Self::Error> {
            if self.fail_read == Some(DefinitionKind::ORDER[file.0 as usize - 1]) {
                let midpoint = bytes.len() / 2;
                bytes[..midpoint].fill(0x37);
                Err("read")
            } else {
                bytes.fill(0x37);
                Ok(())
            }
        }
        fn close(&mut self, _: TableFile) -> Result<(), Self::Error> {
            Ok(())
        }
    }
    struct Controls;
    impl ControlRefBackend for Controls {
        type Error = &'static str;
        fn add_ref(&mut self, _: ControlJoystick) -> Result<(), Self::Error> {
            Ok(())
        }
        fn release(&mut self, _: ControlJoystick) -> Result<(), Self::Error> {
            Ok(())
        }
    }
    impl ControlBindingBackend for Controls {
        fn define_keys(
            &mut self,
            _: ControlGraph,
            _: &mut GameControls,
        ) -> Result<(), Self::Error> {
            Ok(())
        }
        fn set_axis_mode(
            &mut self,
            _: ControlJoystick,
            _: u16,
            _: u32,
            _: &mut GameControls,
        ) -> Result<(), Self::Error> {
            Ok(())
        }
    }
    struct Effects {
        events: Vec<&'static str>,
        choice: i32,
        fail: Option<&'static str>,
    }
    impl Effects {
        fn call(&mut self, name: &'static str) -> Result<(), &'static str> {
            self.events.push(name);
            if self.fail == Some(name) {
                Err(name)
            } else {
                Ok(())
            }
        }
    }
    impl LoadSaveEffects for Effects {
        type Error = &'static str;
        fn all_default(&mut self, profile: &mut ProfileState) -> Result<(), Self::Error> {
            self.call("default")?;
            profile.camera_setting = 1;
            Ok(())
        }
        fn create_indicators(&mut self) -> Result<(), Self::Error> {
            self.call("hud-create")
        }
        fn resolution_changed(&mut self) -> Result<(), Self::Error> {
            self.call("resolution")
        }
        fn clear_indicator_slot_40a4(&mut self) -> Result<(), Self::Error> {
            self.call("hud-clear")
        }
        fn enumerate_profiles(&mut self, profile: &mut ProfileState) -> Result<(), Self::Error> {
            self.call("enumerate")?;
            profile.profiles.push([0x44; 0x54]);
            Ok(())
        }
        fn null_timer(&mut self) -> Result<(), Self::Error> {
            self.call("timer")
        }
        fn open_menu_mission(&mut self) -> Result<(), Self::Error> {
            self.call("mission-menu")
        }
        fn execute_profile_select(&mut self) -> Result<i32, Self::Error> {
            self.call("menu")?;
            Ok(self.choice)
        }
        fn profile_load(&mut self, profile: &mut ProfileState) -> Result<(), Self::Error> {
            self.call("profile-load")?;
            let mut binding = profile.bindings.read(0);
            binding.device = 2;
            binding.code = 1;
            profile.bindings.replace(0, binding);
            Ok(())
        }
        fn profile_save(&mut self, _: &ProfileState) -> Result<(), Self::Error> {
            self.call("profile-save")
        }
        fn destroy_indicators(&mut self) -> Result<(), Self::Error> {
            self.call("hud-destroy")
        }
        fn unload_text(&mut self) -> Result<(), Self::Error> {
            self.call("text-unload")
        }
        fn close_car_database(&mut self) -> Result<(), Self::Error> {
            self.call("car-db-close")
        }
    }
    fn owners() -> (
        GameLoadSave,
        GameDefinitionTables,
        GameControls,
        Tables,
        Controls,
    ) {
        let empty = ControlBinding {
            half: 0,
            device: 0,
            code: 0,
            joystick_index: 0,
        };
        let state = ProfileState {
            init_flag: false,
            race_cars: 0xa5a5_a5a5,
            unknown_1f0: 0xa5a5_a5a5,
            unknown_9c: 0xa5a5_a5a5,
            unknown_1e8: 0xa5a5_a5a5,
            camera_setting: 7,
            bindings: ProfileBindings::from_rows([empty; PROFILE_CONTROL_COUNT]),
            profiles: Vec::new(),
        };
        let seed = ControlAllocationSeed {
            last_input: [0xa5; 12],
            keys: [0xa5; 64],
            key_refs: [0xa5; 64],
            mouse_refs: [0xa5; 4],
            key_mask: 0x5a5a_5a5a_a5a5_a5a5,
            mouse_mask: 0xa5a5_a5a5,
            name: [0xa5; 32],
        };
        let mut controls = GameControls::construct(seed);
        controls
            .init(Some(ControlGraph(1)), &[], &mut Controls)
            .unwrap();
        (
            GameLoadSave::adopt(state, true),
            GameDefinitionTables::default(),
            controls,
            Tables {
                missing: None,
                fail_read: None,
            },
            Controls,
        )
    }
    #[test]
    fn menu_quit_returns_false_after_releasing_profile_list_then_close_skips_save() {
        let (mut owner, mut tables, mut controls, mut files, mut input) = owners();
        let mut effects = Effects {
            events: Vec::new(),
            choice: 0xf,
            fail: None,
        };
        assert_eq!(
            owner.init(
                &mut tables,
                &mut controls,
                &mut files,
                &mut input,
                &mut effects
            ),
            Ok(false)
        );
        assert!(owner.profile.init_flag);
        assert_eq!(owner.profile.camera_setting, 1);
        assert!(owner.profile.profiles.is_empty());
        assert!(owner.profile_ignore_save);
        assert_eq!(controls.binding(59), Some(owner.profile.bindings.read(59)));
        assert_eq!(tables.count(DefinitionKind::Particles), 1);
        owner.close(&mut tables, &mut effects).unwrap();
        assert_eq!(tables.count(DefinitionKind::Particles), 0);
        assert_eq!(
            &effects.events[..],
            [
                "default",
                "hud-create",
                "resolution",
                "hud-clear",
                "enumerate",
                "timer",
                "mission-menu",
                "menu",
                "hud-destroy",
                "text-unload",
                "car-db-close"
            ]
        );
    }
    #[test]
    fn missing_material_file_is_ignored_by_init_and_loaded_profile_is_reapplied() {
        let (mut owner, mut tables, mut controls, mut files, mut input) = owners();
        files.missing = Some(DefinitionKind::Materials);
        let mut effects = Effects {
            events: Vec::new(),
            choice: 0,
            fail: None,
        };
        assert_eq!(
            owner.init(
                &mut tables,
                &mut controls,
                &mut files,
                &mut input,
                &mut effects
            ),
            Ok(true)
        );
        assert_eq!(tables.count(DefinitionKind::Items), 0);
        assert_eq!(tables.table(DefinitionKind::Materials), None);
        assert_eq!(controls.binding(0), Some(owner.profile.bindings.read(0)));
        assert_eq!(owner.profile.bindings.read(0).device, 2);
        assert_eq!(controls.allocation_seed().mouse_refs[0], 1);
        assert!(!owner.profile_ignore_save);
        owner.close(&mut tables, &mut effects).unwrap();
        assert_eq!(
            &effects.events[effects.events.len() - 4..],
            ["profile-save", "hud-destroy", "text-unload", "car-db-close"]
        );
    }
    #[test]
    fn effect_error_keeps_completed_tables_and_controls_then_allows_close() {
        let (mut owner, mut tables, mut controls, mut files, mut input) = owners();
        let mut effects = Effects {
            events: Vec::new(),
            choice: 0,
            fail: Some("resolution"),
        };
        assert_eq!(
            owner.init(
                &mut tables,
                &mut controls,
                &mut files,
                &mut input,
                &mut effects
            ),
            Err(LoadSaveError::Effect("resolution"))
        );
        assert_eq!(effects.events, ["default", "hud-create", "resolution"]);
        assert_eq!(tables.count(DefinitionKind::Particles), 1);
        assert_eq!(controls.binding(59), Some(owner.profile.bindings.read(59)));
        effects.fail = None;
        owner.close(&mut tables, &mut effects).unwrap();
        assert_eq!(tables.count(DefinitionKind::Particles), 0);
    }

    #[test]
    fn read_failure_stops_close_before_effects_and_preserves_pending_file_and_bytes() {
        let (mut owner, mut tables, mut controls, mut files, mut input) = owners();
        files.fail_read = Some(DefinitionKind::Materials);
        let mut effects = Effects {
            events: Vec::new(),
            choice: 0,
            fail: None,
        };
        assert_eq!(
            owner.init(
                &mut tables,
                &mut controls,
                &mut files,
                &mut input,
                &mut effects
            ),
            Err(LoadSaveError::Table(TableLoadError::Backend("read")))
        );
        let partial = tables.table(DefinitionKind::Materials).unwrap().to_vec();
        assert!(partial[..partial.len() / 2]
            .iter()
            .all(|&byte| byte == 0x37));
        assert!(partial[partial.len() / 2..]
            .iter()
            .all(|&byte| byte == 0xa5));
        assert_eq!(tables.pending_file(), Some(TableFile(2)));
        assert_eq!(effects.events, ["default", "hud-create"]);

        assert_eq!(
            owner.close(&mut tables, &mut effects),
            Err(LoadSaveCloseError::Table(TableUnloadError::PendingFile(
                TableFile(2)
            )))
        );
        assert_eq!(effects.events, ["default", "hud-create"]);
        assert_eq!(tables.pending_file(), Some(TableFile(2)));
        assert_eq!(
            tables.table(DefinitionKind::Materials),
            Some(partial.as_slice())
        );
        assert_eq!(
            owner.close(&mut tables, &mut effects),
            Err(LoadSaveCloseError::Interrupted)
        );
    }
}

// ---------------------------------------------------------------------------
// Small G_LoadSaveClass replay, race-car, objective and garage gates.
//
// Source: mafia-re pin 113b5b6, game/ui/g_loadsave.cpp and game/ui/g_loadsave.h.
// Every address below is a Game.exe code entry verified in that reference by its
// `// FUNCTION: GAME 0x...` marker (VA form; RVA = VA - 0x400000). None of these
// RVAs is claimed by docs/rewrite-overlay.json or by the scoped port entries of
// docs/progress.json. Device, file, mission-scene and actor owners stay with the
// callers behind the backend traits; list order and traversal direction are
// preserved exactly.

/// Replay mode words (mafia-re `game/ui/g_loadsave.h:280-285`).
pub const REPLAY_RECORD_START: i32 = -2;
pub const REPLAY_RECORD: i32 = -1;
pub const REPLAY_OFF: i32 = 0;
pub const REPLAY_PLAY: i32 = 1;
/// Fixed replay buffer length in dwords (mafia-re `game/ui/g_loadsave.h:288`).
pub const REPLAY_LENGTH: usize = 0x4000;
/// Low key bits kept from a tick dword (mafia-re `game/ui/g_loadsave.h:291`).
pub const REPLAY_KEYS: u32 = 0x7f;
/// Garage slot count (mafia-re `game/ui/g_loadsave.h:86`).
pub const MAX_GARAGE_CARS: usize = 25;
/// Car descriptor word layout (mafia-re `game/ui/g_loadsave.h:718-720`).
pub const CAR_INDEX_MASK: u32 = 0xffff;
pub const CAR_LEVEL_SHIFT: u32 = 16;
/// Mission-help display time after adding an objective (g_loadsave.cpp:2879).
pub const MISSION_HELP_ADDED_MS: u32 = 5000;
/// Mission-help display time after relisting objectives (g_loadsave.cpp:2902).
pub const MISSION_HELP_LISTED_MS: u32 = 3000;

/// Live replay state (`G_LoadSaveClass` replay members, g_loadsave.h:344 and
/// g_loadsave.cpp:287). The buffer is fixed-length; only the cursor words move.
pub struct LoadSaveReplay {
    pub mode: i32,
    pub unknown_90: bool,
    pub pos: usize,
    pub len: usize,
    pub dword: u32,
    pub buffer: Box<[u32; REPLAY_LENGTH]>,
}

impl LoadSaveReplay {
    pub fn new() -> Self {
        Self {
            mode: REPLAY_OFF,
            unknown_90: false,
            pos: 0,
            len: 0,
            dword: 0,
            buffer: Box::new([0; REPLAY_LENGTH]),
        }
    }
}

impl Default for LoadSaveReplay {
    fn default() -> Self {
        Self::new()
    }
}

/// FUNCTION: GAME RVA 0x20b470, VA 0x60b470 (mafia-re pin 113b5b6,
/// `game/ui/g_loadsave.cpp:2774-2788`, `G_LoadSaveClass::ReplaySet`).
/// Restarts the cursor and the pending dword, then latches record, play or
/// off in that priority order. A started recording keeps length zero until the
/// first tick stores into it.
pub fn replay_set(replay: &mut LoadSaveReplay, record: bool, play: bool) {
    replay.pos = 0;
    replay.dword = 0;
    if record {
        replay.len = 0;
        replay.mode = REPLAY_RECORD_START;
    } else if play {
        replay.mode = REPLAY_PLAY;
    } else {
        replay.mode = REPLAY_OFF;
    }
    replay.unknown_90 = true;
}

/// FUNCTION: GAME RVA 0x20b5a0, VA 0x60b5a0 (mafia-re pin 113b5b6,
/// `game/ui/g_loadsave.cpp:2816-2825`, `G_LoadSaveClass::_replayGetNext`).
/// Returns the dword under the cursor, then advances it with the native clamp:
/// past the recorded length it sticks to the last recorded dword, and past the
/// buffer it sticks to the last slot. A zero length wraps the cursor through
/// the native unsigned arithmetic to the last slot.
pub fn replay_get_next(replay: &mut LoadSaveReplay) -> u32 {
    let value = replay.buffer[replay.pos];
    replay.pos += 1;
    if replay.pos >= replay.len {
        replay.pos = replay.len.wrapping_sub(1);
    }
    if replay.pos >= REPLAY_LENGTH {
        replay.pos = REPLAY_LENGTH - 1;
    }
    value
}

/// FUNCTION: GAME RVA 0x20b5e0, VA 0x60b5e0 (mafia-re pin 113b5b6,
/// `game/ui/g_loadsave.cpp:2828-2835`, `G_LoadSaveClass::_replayPutNext`).
/// Stores the dword under the cursor, advances with the native end clamp, and
/// reports the new cursor as the recorded length.
pub fn replay_put_next(replay: &mut LoadSaveReplay, value: u32) {
    replay.buffer[replay.pos] = value;
    replay.pos += 1;
    if replay.pos >= REPLAY_LENGTH {
        replay.pos = REPLAY_LENGTH - 1;
    }
    replay.len = replay.pos;
}

/// FUNCTION: GAME RVA 0x20b610, VA 0x60b610 (mafia-re pin 113b5b6,
/// `game/ui/g_loadsave.cpp:2838-2841`, `G_LoadSaveClass::RaceCarsEnable`).
/// Sets race-car bits (`G_LoadSaveClass::race_cars`, g_loadsave.h:349).
pub fn race_cars_enable(bits: &mut u32, cars: u32) {
    *bits |= cars;
}

/// FUNCTION: GAME RVA 0x20b630, VA 0x60b630 (mafia-re pin 113b5b6,
/// `game/ui/g_loadsave.cpp:2844-2847`, `G_LoadSaveClass::RaceCarsDisable`).
/// Clears race-car bits.
pub fn race_cars_disable(bits: &mut u32, cars: u32) {
    *bits &= !cars;
}

/// FUNCTION: GAME RVA 0x20b650, VA 0x60b650 (mafia-re pin 113b5b6,
/// `game/ui/g_loadsave.cpp:2850-2853`, `G_LoadSaveClass::RaceCarsGetEnabled`).
/// Returns the race-car bit word.
pub fn race_cars_get_enabled(bits: u32) -> u32 {
    bits
}

/// Car-database owner for the race-car and garage gates. Identity and storage
/// of database rows stay with the caller; only the looked-up words cross here.
pub trait LoadSaveCarDatabase {
    fn race_cars_of(&self, car_index: u32) -> u32;
    fn id_by_params(&self, car: u32, level: u32, color: u32) -> u32;
    fn id_by_actor_car(&self, model: &str, level: u32, color: u32) -> u32;
    fn index_by_model(&self, model: &str) -> u32;
}

/// FUNCTION: GAME RVA 0x20b660, VA 0x60b660 (mafia-re pin 113b5b6,
/// `game/ui/g_loadsave.cpp:2857-2860`, `G_LoadSaveClass::RaceCarsAddWin`).
/// Merges the winning car's database race-car bits into the word. The raw car
/// word (not the garage-masked index) selects the database row, as in the
/// original; row lookup stays with the caller.
pub fn race_cars_add_win(bits: &mut u32, database: &impl LoadSaveCarDatabase, car: u32) {
    *bits |= database.race_cars_of(car);
}

/// Mission-help sink for the objective gates (`mafiaIndicators` calls in
/// `ObjectivesAdd`/`ObjectivesDisplay`); clearing, per-id listing and display
/// time stay with the indicator owner.
pub trait MissionHelpDisplay {
    fn clear_mission_help(&mut self);
    fn add_mission_help(&mut self, id: u32);
    fn set_mission_help_time(&mut self, ms: u32);
}

/// Recorded mission-help call, in call order. Host staging for the HUD side
/// of the objective leaves; applying it to the indicators stays deferred.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MissionHelpCall {
    Clear,
    Add(u32),
    SetTime(u32),
}

/// Recording [`MissionHelpDisplay`]: every call is staged for evidence and
/// later HUD application instead of reaching the indicator owner.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MissionHelpLog {
    pub calls: Vec<MissionHelpCall>,
}

impl MissionHelpDisplay for MissionHelpLog {
    fn clear_mission_help(&mut self) {
        self.calls.push(MissionHelpCall::Clear);
    }

    fn add_mission_help(&mut self, id: u32) {
        self.calls.push(MissionHelpCall::Add(id));
    }

    fn set_mission_help_time(&mut self, ms: u32) {
        self.calls.push(MissionHelpCall::SetTime(ms));
    }
}

/// FUNCTION: GAME RVA 0x20b690, VA 0x60b690 (mafia-re pin 113b5b6,
/// `game/ui/g_loadsave.cpp:2863-2866`, `G_LoadSaveClass::ObjectivesClear`).
/// Drops every objective id.
pub fn objectives_clear(objectives: &mut Vec<u32>) {
    objectives.clear();
}

/// FUNCTION: GAME RVA 0x20b6c0, VA 0x60b6c0 (mafia-re pin 113b5b6,
/// `game/ui/g_loadsave.cpp:2870-2880`, `G_LoadSaveClass::ObjectivesAdd`).
/// Appends an id the list does not have yet, then relists the help and shows
/// it for the added-objective time. A duplicate changes nothing, including the
/// display.
pub fn objectives_add(objectives: &mut Vec<u32>, display: &mut impl MissionHelpDisplay, id: u32) {
    if objectives.iter().any(|&entry| entry == id) {
        return;
    }
    objectives.push(id);
    objectives_display(objectives, &mut *display);
    display.set_mission_help_time(MISSION_HELP_ADDED_MS);
}

/// FUNCTION: GAME RVA 0x20b8a0, VA 0x60b8a0 (mafia-re pin 113b5b6,
/// `game/ui/g_loadsave.cpp:2884-2894`, `G_LoadSaveClass::ObjectivesRemove`).
/// Erases the first matching id ordered, leaving the survivors in place.
/// An id the list does not have changes nothing.
pub fn objectives_remove(objectives: &mut Vec<u32>, id: u32) {
    if let Some(index) = objectives.iter().position(|&entry| entry == id) {
        objectives.remove(index);
    }
}

/// FUNCTION: GAME RVA 0x20b8f0, VA 0x60b8f0 (mafia-re pin 113b5b6,
/// `game/ui/g_loadsave.cpp:2897-2903`, `G_LoadSaveClass::ObjectivesDisplay`).
/// Clears the mission help, lists every objective id in order, and shows the
/// list for the listed-objectives time.
pub fn objectives_display(objectives: &[u32], display: &mut impl MissionHelpDisplay) {
    display.clear_mission_help();
    for &id in objectives {
        display.add_mission_help(id);
    }
    display.set_mission_help_time(MISSION_HELP_LISTED_MS);
}

/// Opaque garage car-actor identity. Native `C_car*` lifetime stays with the
/// caller; `None` is a slot without an actor.
pub type GarageActor = u32;

/// One garage entry (`S_garage_car`, mafia-re `game/ui/g_loadsave.h:239-243`):
/// the database descriptor word (`0` frees the slot) and its actor, if any.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GarageSlot {
    pub car: u32,
    pub actor: Option<GarageActor>,
}

/// Live garage state (`G_LoadSaveClass::garage` and `shown_cars`, mafia-re
/// `game/ui/g_loadsave.h:355-357`).
pub struct LoadSaveGarage {
    pub slots: [GarageSlot; MAX_GARAGE_CARS],
    pub shown_cars: [u32; MAX_GARAGE_CARS],
}

impl LoadSaveGarage {
    pub fn new() -> Self {
        Self {
            slots: [GarageSlot {
                car: 0,
                actor: None,
            }; MAX_GARAGE_CARS],
            shown_cars: [0; MAX_GARAGE_CARS],
        }
    }
}

impl Default for LoadSaveGarage {
    fn default() -> Self {
        Self::new()
    }
}

/// FUNCTION: GAME RVA 0x20b940, VA 0x60b940 (mafia-re pin 113b5b6,
/// `game/ui/g_loadsave.cpp:2906-2914`, `G_LoadSaveClass::GarageClear`).
/// Frees every slot and clears the last-shown cars.
pub fn garage_clear(garage: &mut LoadSaveGarage) {
    for index in 0..MAX_GARAGE_CARS {
        garage.slots[index].car = 0;
        garage.slots[index].actor = None;
        garage.shown_cars[index] = 0;
    }
}

/// FUNCTION: GAME RVA 0x20b970, VA 0x60b970 (mafia-re pin 113b5b6,
/// `game/ui/g_loadsave.cpp:2919-2932`, `G_LoadSaveClass::AddCarByIdx`).
/// Stores the database id (with its level byte cleared) in the first free
/// slot. The car's race-car bits merge into `race_cars` even when the garage
/// is full, as in the original.
pub fn garage_add_car_by_idx(
    garage: &mut LoadSaveGarage,
    race_cars: &mut u32,
    database: &impl LoadSaveCarDatabase,
    car: u32,
    color: u32,
    level: u32,
) {
    for slot in garage.slots.iter_mut() {
        if slot.car == 0 && slot.actor.is_none() {
            slot.car = database.id_by_params(car, level, color) & !(0xff << CAR_LEVEL_SHIFT);
            slot.actor = None;
            break;
        }
    }
    *race_cars |= database.race_cars_of(car & CAR_INDEX_MASK);
}

/// FUNCTION: GAME RVA 0x20b9f0, VA 0x60b9f0 (mafia-re pin 113b5b6,
/// `game/ui/g_loadsave.cpp:2937-2959`, `G_LoadSaveClass::AddCarByActor`).
/// Stores the actor's database id (with its level byte cleared) in the first
/// free slot and keeps the actor. A missing actor changes nothing; a car the
/// garage already keeps under the same actor is not added again. The id lookup
/// runs before the duplicate scan, and the race-car bits merge afterwards, as
/// in the original. Live actor fields cross as plain values; actor lifetime
/// stays with the caller.
pub fn garage_add_car_by_actor(
    garage: &mut LoadSaveGarage,
    race_cars: &mut u32,
    database: &impl LoadSaveCarDatabase,
    actor: Option<GarageActor>,
    model: &str,
    level: u32,
    color: u32,
) {
    let Some(actor) = actor else {
        return;
    };
    let id = database.id_by_actor_car(model, level, color);
    if garage
        .slots
        .iter()
        .any(|slot| slot.car != 0 && slot.actor == Some(actor))
    {
        return;
    }
    for slot in garage.slots.iter_mut() {
        if slot.car == 0 && slot.actor.is_none() {
            slot.car = id & !(0xff << CAR_LEVEL_SHIFT);
            slot.actor = Some(actor);
            break;
        }
    }
    *race_cars |= database.race_cars_of(id & CAR_INDEX_MASK);
}

/// FUNCTION: GAME RVA 0x20baa0, VA 0x60baa0 (mafia-re pin 113b5b6,
/// `game/ui/g_loadsave.cpp:2963-2975`, `G_LoadSaveClass::RemoveCar`).
/// Frees the first slot whose descriptor index matches the model's, whatever
/// its level and variant. The native level parameter takes no part in the
/// match and is kept only for signature parity.
pub fn garage_remove_car(
    garage: &mut LoadSaveGarage,
    database: &impl LoadSaveCarDatabase,
    model: &str,
    _level: i32,
) {
    let index = database.index_by_model(model);
    for slot in garage.slots.iter_mut() {
        if slot.car & CAR_INDEX_MASK == index {
            slot.car = 0;
            slot.actor = None;
            return;
        }
    }
}

/// Script-set garage gates (`G_LoadSaveClass::show_results` at 0x1cc and
/// `add_last_stolen` at 0x1cd, mafia-re `game/ui/g_loadsave.h:358-361`).
/// Cleared by `GarageGameInit`; set by the scripts' `GARAGE_SHOW` and
/// `GARAGE_ADDLASTSTOLEN`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GarageResultFlags {
    pub show_results: bool,
    pub add_last_stolen: bool,
}

/// FUNCTION: GAME RVA 0x20dc80, VA 0x60dc80 (mafia-re pin 113b5b6,
/// `game/ui/g_loadsave.cpp:3618-3622`, `G_LoadSaveClass::GarageAddLastStolen`).
/// Latches the add-last-stolen gate; the stolen car itself moves into the
/// garage in `GarageGameDone`. Verified: `.text` entry stores byte `1` at
/// `+0x1cd` (`mov byte [ecx+0x1cd],1`, `ret`).
pub fn garage_add_last_stolen(flags: &mut GarageResultFlags) {
    flags.add_last_stolen = true;
}

/// FUNCTION: GAME RVA 0x20dc90, VA 0x60dc90 (mafia-re pin 113b5b6,
/// `game/ui/g_loadsave.cpp:3624-3628`, `G_LoadSaveClass::GarageShowResults`).
/// Latches the show-results gate; `GarageGameDone` then runs
/// `GarageShowResultsProc`. Verified: `.text` entry stores byte `1` at
/// `+0x1cc` (`mov byte [ecx+0x1cc],1`, `ret`).
pub fn garage_show_results(flags: &mut GarageResultFlags) {
    flags.show_results = true;
}

/// Added/removed car descriptors since the last shown garage run
/// (`G_LoadSaveClass::GarageShowResultsProc`, `g_loadsave.cpp:3630-3668`).
/// Same-descriptor pairs on both sides cancel out, and the summary menu runs
/// only while either side is non-empty.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GarageResultsDiff {
    pub added: Vec<u32>,
    pub removed: Vec<u32>,
}

impl GarageResultsDiff {
    /// Whether the original runs the summary menu (`added` or `removed`
    /// non-empty at `g_loadsave.cpp:3661`).
    pub fn needs_menu(&self) -> bool {
        !self.added.is_empty() || !self.removed.is_empty()
    }
}

/// FUNCTION: GAME RVA 0x20dca0, VA 0x60dca0 (mafia-re pin 113b5b6,
/// `game/ui/g_loadsave.cpp:3633-3668`,
/// `G_LoadSaveClass::GarageShowResultsProc`): pure diff half. Collects the
/// descriptor words that differ between the garage and the last shown run,
/// cancelling same-descriptor pairs in place (first match wins, the added
/// entry is re-examined, as in the original index loop). The menu itself and
/// the `shown_cars` sync stay with the UI owner. Verified: `.text` FUNCTION
/// entry per the reference marker (its vector SEH frame shape).
pub fn garage_results_diff(garage: &LoadSaveGarage) -> GarageResultsDiff {
    let mut diff = GarageResultsDiff::default();
    for index in 0..MAX_GARAGE_CARS {
        if garage.slots[index].car != garage.shown_cars[index] {
            if garage.slots[index].car != 0 {
                diff.added.push(garage.slots[index].car);
            }
            if garage.shown_cars[index] != 0 {
                diff.removed.push(garage.shown_cars[index]);
            }
        }
    }
    let mut index = 0;
    while index < diff.added.len() {
        if let Some(found) = diff
            .removed
            .iter()
            .position(|&entry| entry == diff.added[index])
        {
            diff.removed.remove(found);
            diff.added.remove(index);
        } else {
            index += 1;
        }
    }
    diff
}

/// FUNCTION: GAME RVA 0x20dee0, VA 0x60dee0 (mafia-re pin 113b5b6,
/// `game/ui/g_loadsave.cpp:3683-3693`, `G_LoadSaveClass::EnableSteal`).
/// ORs the letter bit (`A`/`a` is bit 0) into the steal word; any other byte
/// sets bit 0, as in the original fall-through. Verified: `.text` entry
/// compares the parameter against `'A'` first.
pub fn enable_steal(steal_cars: &mut u32, c: u8) {
    let mut bit = 0;
    if c >= b'A' && c <= b'Z' {
        bit = c - b'A';
    }
    if c >= b'a' && c <= b'z' {
        bit = c - b'a';
    }
    *steal_cars |= 1 << bit;
}

/// FUNCTION: GAME RVA 0x20df60, VA 0x60df60 (mafia-re pin 113b5b6,
/// `game/ui/g_loadsave.cpp:3701-3710`, `G_LoadSaveClass::IsCarIn`). Linear
/// scan of the garage slots for the actor, first slot wins. `None` is the
/// native NULL, compared with plain equality (an empty garage therefore
/// contains NULL, as in the original). Verified: `.text` FUNCTION entry per
/// the reference marker. Live actor lifetime stays with the caller.
pub fn is_car_in(garage: &LoadSaveGarage, car: Option<GarageActor>) -> bool {
    garage.slots.iter().any(|slot| slot.actor == car)
}

#[cfg(test)]
mod loadsave_gates_tests {
    use super::*;

    struct TestCarDatabase {
        race_bits: u32,
    }

    impl LoadSaveCarDatabase for TestCarDatabase {
        fn race_cars_of(&self, _car_index: u32) -> u32 {
            self.race_bits
        }
        fn id_by_params(&self, _car: u32, _level: u32, _color: u32) -> u32 {
            0
        }
        fn id_by_actor_car(&self, _model: &str, _level: u32, _color: u32) -> u32 {
            0
        }
        fn index_by_model(&self, _model: &str) -> u32 {
            0
        }
    }

    struct RecordingCarDatabase {
        id: u32,
        index: u32,
        race_bits: u32,
        race_lookups: std::cell::RefCell<Vec<u32>>,
    }

    impl LoadSaveCarDatabase for RecordingCarDatabase {
        fn race_cars_of(&self, car_index: u32) -> u32 {
            self.race_lookups.borrow_mut().push(car_index);
            self.race_bits
        }
        fn id_by_params(&self, _car: u32, _level: u32, _color: u32) -> u32 {
            self.id
        }
        fn id_by_actor_car(&self, _model: &str, _level: u32, _color: u32) -> u32 {
            self.id
        }
        fn index_by_model(&self, _model: &str) -> u32 {
            self.index
        }
    }

    struct HelpProbe {
        events: Vec<String>,
    }

    /// Database stub that resolves only the `"model"` name to the held index;
    /// any other name misses, which keeps negative remove lookups honest.
    struct ModelIndexDatabase(u32);

    impl LoadSaveCarDatabase for ModelIndexDatabase {
        fn race_cars_of(&self, _car_index: u32) -> u32 {
            0
        }
        fn id_by_params(&self, _car: u32, _level: u32, _color: u32) -> u32 {
            0
        }
        fn id_by_actor_car(&self, _model: &str, _level: u32, _color: u32) -> u32 {
            0
        }
        fn index_by_model(&self, model: &str) -> u32 {
            if model == "model" {
                self.0
            } else {
                0xffff
            }
        }
    }

    impl MissionHelpDisplay for HelpProbe {
        fn clear_mission_help(&mut self) {
            self.events.push("clear".into());
        }
        fn add_mission_help(&mut self, id: u32) {
            self.events.push(format!("add:{id}"));
        }
        fn set_mission_help_time(&mut self, ms: u32) {
            self.events.push(format!("time:{ms}"));
        }
    }

    #[test]
    fn replay_set_rva_20b470_latches_record_play_or_off() {
        let mut replay = LoadSaveReplay::new();
        replay_set(&mut replay, true, true);
        assert_eq!(replay.mode, REPLAY_RECORD_START);
        assert_eq!(replay.len, 0);
        assert_eq!((replay.pos, replay.dword), (0, 0));
        assert!(replay.unknown_90);
        replay_set(&mut replay, false, true);
        assert_eq!(replay.mode, REPLAY_PLAY);
        replay_set(&mut replay, false, false);
        assert_eq!(replay.mode, REPLAY_OFF);
        // Playing keeps a recorded length; only recording resets it.
        replay.len = 7;
        replay_set(&mut replay, false, true);
        assert_eq!(replay.len, 7);
    }

    #[test]
    fn replay_put_next_rva_20b5e0_stores_and_tracks_length_with_end_clamp() {
        let mut replay = LoadSaveReplay::new();
        replay_put_next(&mut replay, 0x11);
        replay_put_next(&mut replay, 0x22);
        assert_eq!(replay.buffer[0], 0x11);
        assert_eq!(replay.buffer[1], 0x22);
        assert_eq!((replay.pos, replay.len), (2, 2));
        replay.pos = REPLAY_LENGTH - 1;
        replay_put_next(&mut replay, 0x33);
        assert_eq!(replay.buffer[REPLAY_LENGTH - 1], 0x33);
        assert_eq!(
            (replay.pos, replay.len),
            (REPLAY_LENGTH - 1, REPLAY_LENGTH - 1)
        );
    }

    #[test]
    fn replay_get_next_rva_20b5a0_reads_then_clamps_to_recorded_tail() {
        let mut replay = LoadSaveReplay::new();
        replay_put_next(&mut replay, 0x11);
        replay_put_next(&mut replay, 0x22);
        replay.pos = 0;
        assert_eq!(replay_get_next(&mut replay), 0x11);
        assert_eq!(replay_get_next(&mut replay), 0x22);
        // Past the recorded length the cursor sticks to the last dword.
        assert_eq!(replay_get_next(&mut replay), 0x22);
        assert_eq!(replay.pos, 1);
    }

    #[test]
    fn replay_get_next_rva_20b5a0_empty_length_wraps_to_last_slot() {
        let mut replay = LoadSaveReplay::new();
        replay.buffer[0] = 0x44;
        assert_eq!(replay_get_next(&mut replay), 0x44);
        // Native unsigned wrap of `len - 1` lands past the buffer end, so the
        // cursor clamps to the last slot instead of underflowing the host.
        assert_eq!(replay.pos, REPLAY_LENGTH - 1);
    }

    #[test]
    fn race_cars_enable_rva_20b610_sets_bits() {
        let mut bits = 0b0100u32;
        race_cars_enable(&mut bits, 0b0011);
        assert_eq!(bits, 0b0111);
    }

    #[test]
    fn race_cars_disable_rva_20b630_clears_bits() {
        let mut bits = 0b0111u32;
        race_cars_disable(&mut bits, 0b0011);
        assert_eq!(bits, 0b0100);
    }

    #[test]
    fn race_cars_get_enabled_rva_20b650_returns_word() {
        assert_eq!(race_cars_get_enabled(0xa5a5), 0xa5a5);
    }

    #[test]
    fn race_cars_add_win_rva_20b660_merges_raw_car_row_bits() {
        let mut bits = 0x01u32;
        let database = TestCarDatabase { race_bits: 0x10 };
        // The raw car word selects the row, exactly as the original passes it.
        race_cars_add_win(&mut bits, &database, 0x0002_0003);
        assert_eq!(bits, 0x11);
    }

    #[test]
    fn objectives_clear_rva_20b690_empties_list() {
        let mut objectives = vec![3u32, 5];
        objectives_clear(&mut objectives);
        assert!(objectives.is_empty());
    }

    #[test]
    fn objectives_add_rva_20b6c0_dedups_then_relists_with_added_time() {
        let mut objectives = vec![3u32];
        let mut display = HelpProbe { events: Vec::new() };
        objectives_add(&mut objectives, &mut display, 5);
        assert_eq!(objectives, vec![3, 5]);
        assert_eq!(
            display.events,
            ["clear", "add:3", "add:5", "time:3000", "time:5000",]
        );
        display.events.clear();
        objectives_add(&mut objectives, &mut display, 3);
        assert_eq!(objectives, vec![3, 5]);
        assert!(display.events.is_empty());
    }

    #[test]
    fn objectives_remove_rva_20b8a0_erases_first_match_in_order() {
        let mut objectives = vec![3u32, 5, 3, 7];
        objectives_remove(&mut objectives, 3);
        assert_eq!(objectives, vec![5, 3, 7]);
        objectives_remove(&mut objectives, 9);
        assert_eq!(objectives, vec![5, 3, 7]);
    }

    #[test]
    fn objectives_display_rva_20b8f0_relists_help_with_listed_time() {
        let mut display = HelpProbe { events: Vec::new() };
        objectives_display(&[3, 5], &mut display);
        assert_eq!(display.events, ["clear", "add:3", "add:5", "time:3000"]);
    }

    #[test]
    fn garage_clear_rva_20b940_zeroes_slots_and_shown_cars() {
        let mut garage = LoadSaveGarage::new();
        garage.slots[0] = GarageSlot {
            car: 9,
            actor: Some(2),
        };
        garage.slots[24] = GarageSlot {
            car: 10,
            actor: None,
        };
        garage.shown_cars[0] = 4;
        garage_clear(&mut garage);
        assert!(garage.slots.iter().all(|slot| *slot
            == GarageSlot {
                car: 0,
                actor: None
            }));
        assert!(garage.shown_cars.iter().all(|&shown| shown == 0));
    }

    #[test]
    fn garage_add_car_by_idx_rva_20b970_uses_first_free_slot_and_always_enables() {
        let database = RecordingCarDatabase {
            // Level byte set: the stored descriptor must clear it.
            id: 0x0002_00c8,
            index: 0,
            race_bits: 0x20,
            race_lookups: std::cell::RefCell::new(Vec::new()),
        };
        let mut garage = LoadSaveGarage::new();
        garage.slots[0] = GarageSlot {
            car: 1,
            actor: None,
        };
        let mut race_cars = 0u32;
        garage_add_car_by_idx(&mut garage, &mut race_cars, &database, 0x0007, 2, 3);
        // The database row is selected by the masked index word.
        assert_eq!(*database.race_lookups.borrow(), vec![0x0007]);
        assert_eq!(garage.slots[1].car, 0x0000_00c8);
        assert_eq!(garage.slots[1].actor, None);
        assert_eq!(race_cars, 0x20);
        // A full garage stores nothing but still merges the race-car bits.
        for slot in garage.slots.iter_mut() {
            *slot = GarageSlot {
                car: 1,
                actor: None,
            };
        }
        race_cars = 0;
        garage_add_car_by_idx(&mut garage, &mut race_cars, &database, 0x0007, 2, 3);
        assert_eq!(race_cars, 0x20);
        assert!(garage.slots.iter().all(|slot| slot.car == 1));
    }

    #[test]
    fn garage_add_car_by_actor_rva_20b9f0_dedups_stores_actor_and_enables() {
        let database = RecordingCarDatabase {
            id: 0x0003_00c8,
            index: 0,
            race_bits: 0x40,
            race_lookups: std::cell::RefCell::new(Vec::new()),
        };
        let mut garage = LoadSaveGarage::new();
        let mut race_cars = 0u32;
        garage_add_car_by_actor(&mut garage, &mut race_cars, &database, None, "model", 1, 2);
        assert!(garage.slots.iter().all(|slot| slot.car == 0));
        assert_eq!(race_cars, 0);
        garage_add_car_by_actor(
            &mut garage,
            &mut race_cars,
            &database,
            Some(11),
            "model",
            1,
            2,
        );
        assert_eq!(
            garage.slots[0],
            GarageSlot {
                car: 0x0000_00c8,
                actor: Some(11)
            }
        );
        assert_eq!(race_cars, 0x40);
        // The enable lookup uses the stored id masked to its index word.
        assert_eq!(*database.race_lookups.borrow(), vec![0x00c8]);
        // The same actor is not added again and merges nothing further.
        let mut race_cars_again = 0u32;
        garage_add_car_by_actor(
            &mut garage,
            &mut race_cars_again,
            &database,
            Some(11),
            "model",
            1,
            2,
        );
        assert_eq!(garage.slots.iter().filter(|slot| slot.car != 0).count(), 1);
        assert_eq!(race_cars_again, 0);
    }

    #[test]
    fn garage_remove_car_rva_20baa0_clears_first_index_match_ignoring_level() {
        let database = ModelIndexDatabase(0x00c8);
        let mut garage = LoadSaveGarage::new();
        // Same index in two slots with different level bytes: only the first
        // match frees, whatever its level and variant.
        garage.slots[0] = GarageSlot {
            car: 0x0001_00c8,
            actor: Some(4),
        };
        garage.slots[1] = GarageSlot {
            car: 0x0002_00c8,
            actor: None,
        };
        garage_remove_car(&mut garage, &database, "model", 7);
        assert_eq!(
            garage.slots[0],
            GarageSlot {
                car: 0,
                actor: None
            }
        );
        assert_eq!(garage.slots[1].car, 0x0002_00c8);
        garage_remove_car(&mut garage, &database, "other", 7);
        assert_eq!(garage.slots[1].car, 0x0002_00c8);
    }

    #[test]
    fn garage_add_last_stolen_rva_20dc80_latches_flag() {
        // G_LoadSaveClass::GarageAddLastStolen GAME RVA 0x20dc80 (VA 0x60dc80).
        let mut flags = GarageResultFlags::default();
        assert!(!flags.add_last_stolen);
        garage_add_last_stolen(&mut flags);
        assert!(flags.add_last_stolen);
        assert!(!flags.show_results);
        garage_add_last_stolen(&mut flags);
        assert!(flags.add_last_stolen);
    }

    #[test]
    fn garage_show_results_rva_20dc90_latches_flag() {
        // G_LoadSaveClass::GarageShowResults GAME RVA 0x20dc90 (VA 0x60dc90).
        let mut flags = GarageResultFlags::default();
        assert!(!flags.show_results);
        garage_show_results(&mut flags);
        assert!(flags.show_results);
        assert!(!flags.add_last_stolen);
    }

    #[test]
    fn garage_results_diff_rva_20dca0_collects_and_cancels_pairs() {
        // G_LoadSaveClass::GarageShowResultsProc GAME RVA 0x20dca0
        // (VA 0x60dca0): pure diff half.
        let mut garage = LoadSaveGarage::new();
        garage.slots[0] = GarageSlot {
            car: 5,
            actor: None,
        };
        garage.slots[1] = GarageSlot {
            car: 7,
            actor: None,
        };
        garage.slots[2] = GarageSlot {
            car: 9,
            actor: None,
        };
        garage.shown_cars[1] = 7;
        garage.shown_cars[2] = 9;
        garage.shown_cars[3] = 11;
        // Slot 0 added, slot 3 removed, slots 1-2 unchanged: no cancel-out.
        let diff = garage_results_diff(&garage);
        assert_eq!(diff.added, vec![5]);
        assert_eq!(diff.removed, vec![11]);
        assert!(diff.needs_menu());
        // A car on both sides under the same descriptor cancels out.
        garage.shown_cars[0] = 5;
        garage.slots[3] = GarageSlot {
            car: 11,
            actor: None,
        };
        let cancelled = garage_results_diff(&garage);
        assert!(cancelled.added.is_empty());
        assert!(cancelled.removed.is_empty());
        assert!(!cancelled.needs_menu());
        // An empty garage against an empty run needs no menu either.
        assert!(!garage_results_diff(&LoadSaveGarage::new()).needs_menu());
    }

    #[test]
    fn enable_steal_rva_20dee0_maps_letter_to_bit() {
        // G_LoadSaveClass::EnableSteal GAME RVA 0x20dee0 (VA 0x60dee0).
        let mut steal_cars = 0u32;
        enable_steal(&mut steal_cars, b'A');
        assert_eq!(steal_cars, 1);
        enable_steal(&mut steal_cars, b'a');
        assert_eq!(steal_cars, 1);
        enable_steal(&mut steal_cars, b'C');
        assert_eq!(steal_cars, 0b101);
        enable_steal(&mut steal_cars, b'z');
        assert_eq!(steal_cars, 0b101 | (1 << 25));
        // A non-letter falls through to bit 0.
        let mut other = 0u32;
        enable_steal(&mut other, b'0');
        assert_eq!(other, 1);
    }

    #[test]
    fn is_car_in_rva_20df60_scans_garage_actors() {
        // G_LoadSaveClass::IsCarIn GAME RVA 0x20df60 (VA 0x60df60).
        let mut garage = LoadSaveGarage::new();
        garage.slots[2] = GarageSlot {
            car: 8,
            actor: Some(42),
        };
        assert!(is_car_in(&garage, Some(42)));
        assert!(!is_car_in(&garage, Some(43)));
        // A garage whose slots are all occupied holds no NULL.
        let mut full = LoadSaveGarage::new();
        for slot in full.slots.iter_mut() {
            slot.actor = Some(1);
        }
        assert!(!is_car_in(&full, None));
        // NULL compares equal to empty slots, as in the original.
        assert!(is_car_in(&LoadSaveGarage::new(), None));
    }
}
