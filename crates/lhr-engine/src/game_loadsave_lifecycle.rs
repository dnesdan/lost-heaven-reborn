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
use crate::itexture::TextureKey;
use crate::load_scene_callback::LoadingScreenPresentBackend;
use crate::menu::MenuComponentPrecision;
use crate::menu_draw_resources::{MenuDrawResourceAccess, MenuDrawResources};
use crate::menu_native_drawing::{
    MenuNativeDrawBackend, MenuNativeDrawState, MenuNativeMenu, MenuNativeViewport,
};
use crate::menu_text_drawing::MenuTextPrecision;

/// Profile unlock words, independent of `FreerideState::location` (the
/// selected location, including when supplied by the CLI probe).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FreerideProfile {
    /// `G_LoadSaveClass::freeride_enabled` (+0x1f0), the unlocked area level.
    pub freeride_enabled: u32,
    /// `G_LoadSaveClass::race_cars`, the unlocked race-car bitmask.
    pub race_cars: u32,
}

impl FreerideProfile {
    pub(crate) fn freeride_setup_from_script(
        &self,
        selection: &FreerideState,
        tuning: &mut crate::traffic_car_actor::TrafficTuning,
        ratios: &mut crate::traffic_generator::TrafficGeneratorRatios,
        steal_cars: &mut u32,
    ) {
        if self.freeride_enabled >= crate::main_menu::FREERIDE_EXTREME {
            let traffic = selection.traffic_density.to_bits();
            let police = selection.police_density.to_bits();
            tuning.set_elems_coeff_bits(traffic);
            let pedestrians = selection.pedestrian_density.to_bits();
            tuning.set_police_coeff_bits(police);
            ratios.all_bits = pedestrians;
            ratios.police_bits = police;
        }
        *steal_cars = self.race_cars;
    }

    /// The two profile words left by `G_LoadSaveClass::Init` (GAME 0x604ee0)
    /// on a confirmed fresh installation. The Init head clears both words;
    /// `AllDefault` (GAME 0x6051e0) leaves `freeride_enabled = 0` and
    /// `race_cars = 1`. This is not the full `AllDefault` or `ProfileLoad`.
    pub fn after_load_save_init(intake: ProfileFileIntake) -> Result<Self, &'static str> {
        match intake {
            ProfileFileIntake::NoProfileFile => Ok(Self {
                freeride_enabled: 0,
                race_cars: 1,
            }),
            ProfileFileIntake::ProfileFilePresent => Err(
                "G_LoadSaveClass::ProfileLoad (GAME 0x6057f0) reads an existing savegame\\mafiaNNN.sav through G_Stream, which has no owner",
            ),
        }
    }
}

/// Fields directly read or written by Init, Close, ApplyControls and Reset.
/// Other profile bytes live in the eventual profile file owner; this structure
/// does not claim a mirrored C++ class layout.
pub struct ProfileState {
    pub init_flag: bool,
    pub freeride_profile: FreerideProfile,
    pub unknown_9c: u32,
    pub camera_setting: u32,
    pub bindings: ProfileBindings,
    pub profiles: Vec<[u8; 0x54]>,
}
impl ProfileState {
    /// The `G_LoadSaveClass::Init` head stores (GAME 0x604ee0,
    /// g_loadsave.cpp:110-114). Its fifth store, `last_stolen = 0` at
    /// +0x1e8, writes the field [`LoadSaveProgress::last_stolen`] owns, so it
    /// lands there rather than in a duplicate profile word.
    fn begin_initialization(&mut self, progress: &mut LoadSaveProgress) {
        self.init_flag = true;
        self.freeride_profile.race_cars = 0;
        self.freeride_profile.freeride_enabled = 0;
        self.unknown_9c = 0;
        progress.last_stolen = None;
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
/// The HUD calls route through `crate::indicators::IndicatorsLoadSave`:
/// the texture and font prefix belongs to `MenuDrawResources`, and the
/// direct slot clear belongs to `IndicatorsState`. `create_indicators` is
/// `G_IndicatorsClass::Create` (GAME 0x5f70e0, `g_indicators.cpp:256`),
/// `clear_indicator_slot_40a4` is the direct `show_flags = 0` store at member
/// 0x40a4 (`G_LoadSaveClass::Init` GAME 0x604ee0, `g_loadsave.cpp:120`), and
/// `destroy_indicators` is `G_IndicatorsClass::Destroy` (GAME 0x5f7250,
/// `g_indicators.cpp:298`).
pub trait LoadSaveEffects {
    type Error;
    /// `AllDefault` (GAME 0x6051e0). `settings` is the single
    /// `mafiaLoadSave.settings` owner ([`LoadSaveProgress::settings`]); its
    /// `DefaultSettings(0xffff)` call writes the represented options there.
    fn all_default(
        &mut self,
        profile: &mut ProfileState,
        settings: &mut Option<ProfileSettings>,
    ) -> Result<(), Self::Error>;
    /// HUD startup `G_IndicatorsClass::Create` (GAME 0x5f70e0).
    fn create_indicators(&mut self) -> Result<(), Self::Error>;
    fn resolution_changed(&mut self) -> Result<(), Self::Error>;
    /// HUD owner slot clear: `IndicatorsState::clear_flags_slot` (direct
    /// `show_flags` member 0x40a4 zero store, `g_loadsave.cpp:120`).
    fn clear_indicator_slot_40a4(&mut self) -> Result<(), Self::Error>;
    fn enumerate_profiles(&mut self, profile: &mut ProfileState) -> Result<(), Self::Error>;
    fn null_timer(&mut self) -> Result<(), Self::Error>;
    fn open_menu_mission(&mut self) -> Result<(), Self::Error>;
    fn execute_profile_select(&mut self) -> Result<i32, Self::Error>;
    /// `ProfileLoad` (GAME 0x6057f0): the chosen profile's options land in
    /// the same `settings` owner (or `AllDefault`'s, when the read fails).
    fn profile_load(
        &mut self,
        profile: &mut ProfileState,
        settings: &mut Option<ProfileSettings>,
    ) -> Result<(), Self::Error>;
    fn profile_save(&mut self, profile: &ProfileState) -> Result<(), Self::Error>;
    /// HUD teardown `G_IndicatorsClass::Destroy` (GAME 0x5f7250).
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
        progress: &mut LoadSaveProgress,
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
        self.profile.begin_initialization(progress);
        init_effect(
            &mut self.init_interrupted,
            effects.all_default(&mut self.profile, &mut progress.settings),
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
            effects.profile_load(&mut self.profile, &mut progress.settings),
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
        fn all_default(
            &mut self,
            profile: &mut ProfileState,
            settings: &mut Option<ProfileSettings>,
        ) -> Result<(), Self::Error> {
            self.call("default")?;
            profile.camera_setting = 1;
            *settings = Some(ProfileSettings {
                speedometer_mph: true,
            });
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
        fn profile_load(
            &mut self,
            profile: &mut ProfileState,
            settings: &mut Option<ProfileSettings>,
        ) -> Result<(), Self::Error> {
            self.call("profile-load")?;
            *settings = Some(ProfileSettings {
                speedometer_mph: false,
            });
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
            freeride_profile: FreerideProfile {
                freeride_enabled: 0xa5a5_a5a5,
                race_cars: 0xa5a5_a5a5,
            },
            unknown_9c: 0xa5a5_a5a5,
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
    fn fresh_freeride_profile_uses_all_default_instead_of_init_head_stores() {
        let (mut owner, _, _, _, _) = owners();
        owner
            .profile
            .begin_initialization(&mut LoadSaveProgress::default());
        assert_eq!(
            owner.profile.freeride_profile,
            FreerideProfile {
                freeride_enabled: 0,
                race_cars: 0,
            }
        );
        assert_eq!(
            FreerideProfile::after_load_save_init(ProfileFileIntake::NoProfileFile),
            Ok(FreerideProfile {
                freeride_enabled: 0,
                race_cars: 1,
            })
        );
    }

    #[test]
    fn freeride_profile_refuses_a_present_profile_file() {
        assert!(
            FreerideProfile::after_load_save_init(ProfileFileIntake::ProfileFilePresent)
                .unwrap_err()
                .contains("ProfileLoad (GAME 0x6057f0)")
        );
    }

    #[test]
    fn menu_quit_returns_false_after_releasing_profile_list_then_close_skips_save() {
        let (mut owner, mut tables, mut controls, mut files, mut input) = owners();
        let mut progress = LoadSaveProgress {
            last_stolen: Some(9),
            ..Default::default()
        };
        let mut effects = Effects {
            events: Vec::new(),
            choice: 0xf,
            fail: None,
        };
        assert_eq!(
            owner.init(
                &mut progress,
                &mut tables,
                &mut controls,
                &mut files,
                &mut input,
                &mut effects
            ),
            Ok(false)
        );
        assert!(owner.profile.init_flag);
        // The +0x1e8 `last_stolen = 0` store lands in its single owner.
        assert_eq!(progress.last_stolen, None);
        assert_eq!(owner.profile.camera_setting, 1);
        // `AllDefault` writes the options through the composition into the
        // single `mafiaLoadSave.settings` owner; the quit leaves them.
        assert_eq!(
            progress.settings,
            Some(ProfileSettings {
                speedometer_mph: true
            })
        );
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
        let mut progress = LoadSaveProgress {
            last_stolen: Some(9),
            ..Default::default()
        };
        files.missing = Some(DefinitionKind::Materials);
        let mut effects = Effects {
            events: Vec::new(),
            choice: 0,
            fail: None,
        };
        assert_eq!(
            owner.init(
                &mut progress,
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
        // `ProfileLoad`'s options replace `AllDefault`'s in the same owner.
        assert_eq!(
            progress.settings,
            Some(ProfileSettings {
                speedometer_mph: false
            })
        );
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
        let mut progress = LoadSaveProgress {
            last_stolen: Some(9),
            ..Default::default()
        };
        let mut effects = Effects {
            events: Vec::new(),
            choice: 0,
            fail: Some("resolution"),
        };
        assert_eq!(
            owner.init(
                &mut progress,
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
        let mut progress = LoadSaveProgress {
            last_stolen: Some(9),
            ..Default::default()
        };
        files.fail_read = Some(DefinitionKind::Materials);
        let mut effects = Effects {
            events: Vec::new(),
            choice: 0,
            fail: None,
        };
        assert_eq!(
            owner.init(
                &mut progress,
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
/// The session's startup CarDatabase does not implement these remaining native
/// lookups yet; the interface has only test implementations, not another owner.
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
    if objectives.contains(&id) {
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
#[derive(Clone, Debug, Eq, PartialEq)]
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
    if c.is_ascii_uppercase() {
        bit = c - b'A';
    }
    if c.is_ascii_lowercase() {
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

/// The represented live `G_LoadSaveClass` fields (`mafiaLoadSave`, GAME
/// 0x6d4628; mafia-re `fe7e746` `game/ui/g_loadsave.h:350-382`): the
/// progress a new game resets, `steal_cars` (+0x98), the garage (+0xa0
/// slots, +0x168 `shown_cars`) and `last_stolen` (+0x1e8), and the loading
/// screen pointer `progress` (+0x5d4), and the options' `speedometer_mph`
/// byte (`settings`, +0x4c4+0x58). The member names are mafia-re's, not
/// original symbols; the class name is the PS2 one. This is not a mirrored
/// layout of the class: profiles, the other options, bindings and the
/// replay stay with their own owners. Every represented `mafiaLoadSave` field has this
/// single owner; `G_LoadSaveClass::Init`'s +0x1e8 store reaches it through
/// [`GameLoadSave::init`].
///
/// The default is the process-start state of the global: static storage is
/// zero and the constructor (GAME 0x605140, g_loadsave.cpp:169-179, per
/// mafia-re source; its body was not re-read here) stores none of these
/// fields.
#[derive(Debug, Default, PartialEq)]
pub struct LoadSaveProgress {
    /// +0x98: the cars `EnableSteal` lets steal, a bit per letter.
    pub steal_cars: u32,
    /// +0xa0 / +0x168: the garage slots and the last-shown cars.
    pub garage: LoadSaveGarage,
    /// +0x1e8: the car the player stole last (`C_car*`), `None` for NULL.
    pub last_stolen: Option<GarageActor>,
    /// +0x5d4: the mission's loading screen (`GM_LoadProgress*`), `None` for
    /// NULL. Natively `C_mission::Open` allocates it when asked for progress
    /// and none exists (Mission.cpp:640-641), `C_game::Init` drives it
    /// through `_show_progress_` and deletes it after its last mark
    /// (game.cpp:971-976, unported). `SaveGameNewGame` leaves it alone.
    pub progress: Option<LoadProgressScreen>,
    /// +0x4c4: the profile's options (`S_profile_settings`, 0x5c bytes),
    /// represented only by the byte a reached operation reads. `None` until
    /// `G_LoadSaveClass::Init`'s profile intake has set them for this
    /// process (see [`ProfileSettings`]); the constructor's zero bytes
    /// (GAME 0x605140 `memset`) are never read by a reached operation,
    /// because natively `Init` runs before any game starts.
    pub settings: Option<ProfileSettings>,
}

/// `mafiaLoadSave.settings` (`S_profile_settings` at +0x4c4; mafia-re
/// `fe7e746` `game/ui/g_loadsave.h:197-233`, the struct and member names are
/// mafia-re's), represented only by `speedometer_mph` (+0x58, absolute GAME
/// 0x6d4b44), which `G_IndicatorsClass::GameInit` reads (`mov al,
/// [0x6d4b44]` at 0x5f73e4/0x5f7441). The other options (the camera words
/// among them, which [`ProfileState::camera_setting`] carries for the
/// startup adapter) stay unrepresented here: no reached operation reads
/// them, so they acquire no second owner.
///
/// Natively the bytes come from `G_LoadSaveClass::Init` (GAME 0x604ee0)
/// before the main menu: `AllDefault` (0x6051e0) calls
/// `DefaultSettings(0xffff)` (`push 0xffff` at 0x6051fa, `call 0x607ab0`
/// at 0x6051ff), whose `DEFAULTS_OTHER` arm stores `mov [edx+0x51c], bl`
/// (0x607ae4, `bl = 1` from `mov bl, 1` at 0x607ad8: `speedometer_mph =
/// true`); then `ProfileLoad` (0x6057f0) reads the
/// chosen profile's `savegame\mafiaNNN.sav` (G_Stream, unported) and, when
/// that read fails, calls `AllDefault` again (g_loadsave.cpp:344-345).
/// Only the options menu (`g_m_func.cpp:720`, unported) changes the byte
/// between that intake and a new game.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProfileSettings {
    /// +0x58: the speedometer shows miles ("Miles per Hour"). Natively a
    /// byte read with `test al, al`: a future `ProfileLoad` decode must map
    /// any non-zero byte to true, not only 1.
    pub speedometer_mph: bool,
}

/// What `G_LoadSaveClass::Init`'s `ProfileLoad` found for the chosen
/// profile, as the host observes the installation's `savegame` directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileFileIntake {
    /// No `savegame\mafiaNNN.sav` exists: every profile's `OpenRead`
    /// fails, `load_failed` stays set and `AllDefault` supplies the
    /// profile defaults (a fresh installation).
    NoProfileFile,
    /// A profile file exists; its stored values would come from the G_Stream
    /// read, which has no owner yet.
    ProfileFilePresent,
}

/// The represented options `G_LoadSaveClass::Init` leaves for a new game,
/// from the profile intake. A present profile file is refused: its
/// G_Stream decode (`ProfileLoad`, GAME 0x6057f0) is unported, and
/// defaults are never substituted for its bytes. This descriptive helper
/// receives no function credit (`AllDefault`/`DefaultSettings` write many
/// unrepresented options).
pub fn profile_settings_after_load_save_init(
    intake: ProfileFileIntake,
) -> Result<ProfileSettings, &'static str> {
    match intake {
        // AllDefault -> DefaultSettings(0xffff): byte +0x51c = 1 (0x607ae4).
        ProfileFileIntake::NoProfileFile => Ok(ProfileSettings {
            speedometer_mph: true,
        }),
        ProfileFileIntake::ProfileFilePresent => Err(
            "G_LoadSaveClass::ProfileLoad (GAME 0x6057f0) reads an existing savegame\\mafiaNNN.sav through G_Stream, which has no owner",
        ),
    }
}

/// The `GM_LoadProgress` loading screen behind `mafiaLoadSave.progress`
/// (PS2 class name; mafia-re `fe7e746` `game/ui/g_m_func.h:222-258`, VTABLE
/// GAME 0x6275c0), also the screen `cbLoadScene` creates for itself
/// (`menuProgress`, [`crate::load_scene_callback`]). Holds the constructor's
/// member stores (GAME 0x5e3aa0, g_m_func.cpp:991-1008): `mission_part`
/// (+0x30), `title` (+0x38) and `mission_name` (+0x3c, NULL for the active
/// mission's), and the `GM_Menu` base's component stack. The constructor's
/// Runtime composition uses an explicit host layout and a shared-resource
/// receipt. The constructor's display-scale calculation is a separate input
/// boundary.
///
/// `menu` is empty from the constructor until [`LoadProgressScreen::create`]
/// (`GM_Menu::Create`, natively from `cbLoadScene`'s `LOAD_PROGRESS_BEGIN`)
/// builds the `loadprogress` components. Before that the bar write
/// (`SetFloat(LOADPROGRESS_BAR)`) finds no component and is the native
/// no-op of `GM_Menu::SetFloat` over an empty component list.
///
/// A loading screen keeps its menu handle private so inspection cannot detach
/// its component resources from the screen that destroys them.
///
/// ```compile_fail,E0616
/// use lhr_engine::mission_load_session::MissionLoadSession;
///
/// fn retain_menu(session: &MissionLoadSession) {
///     let screen = session.load_save_progress().progress.as_ref().unwrap();
///     let _alias = screen.menu.clone();
/// }
/// ```
#[derive(Debug)]
pub struct LoadProgressScreen {
    /// +0x38: text-database title shown with "press <ESC> to Abort", 0 for
    /// none (mafia-re's name).
    pub title: u32,
    /// +0x30: the bar's part the mission's opening fills (mafia-re's name).
    pub mission_part: f32,
    /// +0x3c: the mission whose picture and name `OnCreate` shows, `None`
    /// (NULL) for `act_mission`'s (mafia-re's name).
    pub mission_name: Option<Vec<u8>>,
    /// The `GM_Menu` base's components (+0x10) with their staged values.
    menu: crate::menu_native_drawing::MenuNativeMenu,
    shared_instance: Option<crate::menu_shared_resources::MenuSharedInstance>,
}

struct LoadingMenuDraw<'a> {
    device: &'a mut dyn LoadingScreenPresentBackend,
    resources: Option<&'a dyn MenuDrawResourceAccess>,
    resolve_progress_texture: &'a dyn Fn() -> Result<Option<TextureKey>, String>,
    text_precision: MenuTextPrecision,
}

impl MenuNativeDrawBackend for LoadingMenuDraw<'_> {
    fn set_z_buffer(&mut self, enabled: bool) -> Result<(), String> {
        self.device.set_z_buffer(enabled)
    }

    fn screen_height(&mut self) -> Result<u32, String> {
        self.device.screen_height()
    }

    fn screen_width(&mut self) -> Result<u32, String> {
        self.device.screen_width()
    }

    fn set_viewport(&mut self, viewport: MenuNativeViewport) -> Result<(), String> {
        self.device.set_viewport(viewport)
    }

    fn draw_components(
        &mut self,
        menu: &MenuNativeMenu,
        draw_flags: u32,
        _: Option<&TextureKey>,
    ) -> Result<(), String> {
        crate::menu_rendering::submit_menu_native_components(
            menu,
            draw_flags,
            self.resolve_progress_texture,
            self.resources,
            self.text_precision,
            &mut |mesh| self.device.draw_menu_mesh(mesh),
        )
    }

    fn draw_cursor(&mut self, x: f32, y: f32) -> Result<(), String> {
        self.device.draw_cursor(x, y)
    }
}

impl PartialEq for LoadProgressScreen {
    fn eq(&self, other: &Self) -> bool {
        self.title == other.title
            && self.mission_part == other.mission_part
            && self.mission_name == other.mission_name
            && self.menu.identity() == other.menu.identity()
    }
}

/// Mission-open loading screen arguments: `new GM_LoadProgress(0, .7f,
/// NULL)` (Mission.cpp:641; GAME 0x5409d0 pushes title 0, mission_part
/// 0x3f333333 and NULL).
pub const MISSION_OPEN_PROGRESS_PART: f32 = 0.7;

/// `tables\load.def` (GAME string 0x638368) as `OnCreate` opens it.
pub const LOAD_DEF_PATH: &str = "tables/load.def";
/// `OnCreate`'s `new char[8000]` buffer and `dtaRead` limit (`push 0x1f40`
/// at 0x5e3bd6 and 0x5e3c03).
pub const LOAD_DEF_BUFFER: usize = 8000;
/// `sizeof(S_load_def)`: `read / 0x44` through the `0xf0f0f0f1` reciprocal
/// at 0x5e3c15-0x5e3c20, entries stepped by `add ebp, 0x44` (0x5e3c45).
pub const LOAD_DEF_ENTRY: usize = 0x44;

/// The `loadprogress` items `OnCreate` addresses (mafia-re
/// `g_m_func.cpp:51-60` names; the ids are the pushed immediates).
pub const LOADPROGRESS_SIDE: u32 = 0x64;
pub const LOADPROGRESS_MISSION_PICTURE: u32 = 0x65;
pub const LOADPROGRESS_LOGO: u32 = 0x66;
pub const LOADPROGRESS_MISSION_NAME: u32 = 0xc8;
pub const LOADPROGRESS_UNKNOWN_1000: u32 = 1000;
pub const LOADPROGRESS_UNKNOWN_2000: u32 = 2000;
pub const LOADPROGRESS_UNKNOWN_3000: u32 = 3000;

/// The inputs `GM_Menu::Create` and `GM_LoadProgress::OnCreate` read while
/// building the screen, each read when the original reads it.
pub trait LoadProgressCreateInputs {
    fn shared_lifecycle(&mut self) -> Option<&mut dyn LoadProgressSharedLifecycle> {
        None
    }

    fn native_layout(&self) -> Option<crate::menu::MenuComponentCreateContext> {
        None
    }
    fn component_resources(
        &mut self,
    ) -> Option<&mut dyn crate::menu_draw_resources::MenuComponentResources> {
        None
    }
    fn draw_resources(&self) -> Option<&crate::menu_draw_resources::MenuDrawResources> {
        None
    }
    fn progress_texture(&self) -> Result<Option<crate::itexture::TextureKey>, String> {
        Ok(None)
    }
    /// `CreateCompsByDef(name)` file intake (GAME 0x5ea370): the language's
    /// `tables\menu\<name>_<ll>.mnu`, then `<name>.mnu`. `Ok(None)` is the
    /// native "no file" early return (no components); an unparseable file
    /// or an unavailable language owner is `Err`.
    fn menu_definition(&mut self, name: &str)
        -> Result<Option<lhr_assets::MenuDefinition>, String>;
    /// `dtaOpen("tables\\load.def")` + `dtaRead(.., 8000)`: `Ok(None)` for
    /// the failed open (`jl` at 0x5e3bfb), else the read bytes.
    fn load_def(&mut self) -> Result<Option<Vec<u8>>, String>;
    /// `act_mission->mission_name` (`[0x63788c]+0x68`), read per compared
    /// entry while the screen's own `mission_name` is NULL.
    fn act_mission_name(&mut self) -> Result<Vec<u8>, String>;
}

pub trait LoadProgressSharedLifecycle {
    fn construct_shared(
        &mut self,
    ) -> Result<crate::menu_shared_resources::MenuSharedInstance, String>;
    fn check_shared_instance(
        &self,
        instance: &crate::menu_shared_resources::MenuSharedInstance,
    ) -> Result<(), String>;
    fn check_shared(
        &self,
        instance: &crate::menu_shared_resources::MenuSharedInstance,
    ) -> Result<(), String>;
    fn destroy_shared(
        &mut self,
        instance: &mut crate::menu_shared_resources::MenuSharedInstance,
    ) -> Result<(), String>;
    fn play_open_sound(&mut self, closing: bool) -> Result<(), String>;
}

struct LoadProgressComponentResources<'a> {
    inputs: &'a mut dyn LoadProgressCreateInputs,
    unavailable: &'static str,
}

impl crate::menu_draw_resources::MenuComponentResources for LoadProgressComponentResources<'_> {
    fn text(&mut self, text_id: u32) -> Result<Option<std::sync::Arc<[u8]>>, String> {
        self.inputs
            .component_resources()
            .ok_or(self.unavailable)?
            .text(text_id)
    }

    fn create_alpha_texture(
        &mut self,
        name: &[u8],
    ) -> Result<Option<crate::itexture::TextureKey>, String> {
        self.inputs
            .component_resources()
            .ok_or(self.unavailable)?
            .create_alpha_texture(name)
    }

    fn release_texture(&mut self, key: crate::itexture::TextureKey) -> Result<(), String> {
        self.inputs
            .component_resources()
            .ok_or(self.unavailable)?
            .release_texture(key)
    }
}

/// `_stricmp` equality of the C string at `field` inside the read `bytes`
/// and `name` (whose end is its NUL), folding ASCII case as the CRT's "C"
/// locale. A comparison that would read past the read bytes is refused
/// (natively it continues into the rest of the uninitialized buffer).
fn stricmp_eq(bytes: &[u8], field: usize, name: &[u8]) -> Result<bool, String> {
    let mut index = 0;
    loop {
        let left = *bytes
            .get(field + index)
            .ok_or("load.def mission string runs past the read bytes")?;
        let right = name.get(index).copied().unwrap_or(0);
        if !left.eq_ignore_ascii_case(&right) {
            return Ok(false);
        }
        if left == 0 {
            return Ok(true);
        }
        index += 1;
    }
}

/// The C string at `field` inside the read `bytes`, without its NUL.
fn c_string_at(bytes: &[u8], field: usize) -> Result<&[u8], String> {
    let tail = bytes
        .get(field..)
        .ok_or("load.def picture outside the read bytes")?;
    let end = tail
        .iter()
        .position(|&byte| byte == 0)
        .ok_or("load.def picture runs past the read bytes")?;
    Ok(&tail[..end])
}

impl LoadProgressScreen {
    /// `GM_LoadProgress(title, mission_part, mission_name)` member stores
    /// (GAME 0x5e3aa0) over an empty `GM_Menu` base.
    pub fn new(title: u32, mission_part: f32, mission_name: Option<Vec<u8>>) -> Self {
        Self {
            title,
            mission_part,
            mission_name,
            menu: Self::empty_menu(),
            shared_instance: None,
        }
    }

    fn empty_menu() -> crate::menu_native_drawing::MenuNativeMenu {
        crate::menu_native_drawing::MenuNativeMenu::new(
            crate::menu::MenuStack::new(),
            None,
            crate::menu_native_drawing::MENUF_DRAW_PARENT,
        )
    }

    pub(crate) fn construct_for_load(
        mission_part: f32,
        inputs: &mut dyn LoadProgressCreateInputs,
    ) -> Result<Self, String> {
        let menu = Self::empty_menu();
        let shared_instance = inputs
            .shared_lifecycle()
            .map(LoadProgressSharedLifecycle::construct_shared)
            .transpose()?;
        Ok(Self {
            title: 0,
            mission_part,
            mission_name: None,
            menu,
            shared_instance,
        })
    }

    pub(crate) fn has_shared_instance(&self) -> bool {
        self.shared_instance.is_some()
    }

    /// `C_mission::Open`'s screen (Mission.cpp:640-641): title 0, the
    /// opening's 0.7 of the bar, the active mission's name.
    pub fn for_mission_open() -> Self {
        Self::new(0, MISSION_OPEN_PROGRESS_PART, None)
    }

    /// `cbLoadScene`'s own screen: `GM_LoadProgress(0, 1.0f, NULL)` (GAME
    /// 0x5bcd68-0x5bcd73 pushes 0, 0x3f800000 and 0).
    pub fn for_load_callback() -> Self {
        Self::new(0, 1.0, None)
    }

    /// Host inspection of the actual progress-bar slot. Native `GetFloat`
    /// remains the base zero result; the pure staged subset keeps its getter.
    pub fn raw_progress(&self) -> Result<f32, String> {
        self.menu
            .with_components(|menu| {
                Ok(menu
                    .progress_value(crate::menu::LOADPROGRESS_BAR_ID)
                    .unwrap_or_else(|| menu.get_float(crate::menu::LOADPROGRESS_BAR_ID)))
            })
            .map_err(|error| error.to_string())
    }

    pub fn uses_native_components(&self) -> Result<bool, String> {
        self.menu
            .with_components(|menu| Ok(menu.has_native_lifecycle()))
            .map_err(|error| error.to_string())
    }

    pub fn component_count(&self) -> Result<usize, String> {
        self.menu
            .with_components(|menu| Ok(menu.len()))
            .map_err(|error| error.to_string())
    }

    /// Copies the staged picture name, distinguishing an unset value from a
    /// staged NULL. The bytes carry no component or resource ownership.
    pub fn staged_mission_picture(&self) -> Result<Option<Option<Vec<u8>>>, String> {
        self.menu
            .with_components(|menu| {
                Ok(menu
                    .staged_string(LOADPROGRESS_MISSION_PICTURE)
                    .map(|picture| picture.map(<[u8]>::to_vec)))
            })
            .map_err(|error| error.to_string())
    }

    pub(crate) fn draw_for_present(
        &self,
        globals: &MenuNativeDrawState,
        resolve_progress_texture: &dyn Fn() -> Result<Option<TextureKey>, String>,
        precision: MenuComponentPrecision,
        resources: Option<&MenuDrawResources>,
        text_precision: MenuTextPrecision,
        device: &mut dyn LoadingScreenPresentBackend,
    ) -> Result<(), String> {
        crate::menu_native_drawing::draw_menu_native(
            &self.menu,
            globals,
            u32::MAX,
            None,
            precision,
            &mut LoadingMenuDraw {
                device,
                resources: resources.map(|resources| resources as &dyn MenuDrawResourceAccess),
                resolve_progress_texture,
                text_precision,
            },
        )
        .map_err(|error| error.to_string())
    }

    pub(crate) fn draw_components(
        &self,
        draw_flags: u32,
        resolve_progress_texture: &dyn Fn() -> Result<Option<TextureKey>, String>,
        resources: &MenuDrawResources,
        text_precision: MenuTextPrecision,
    ) -> crate::menu_rendering::MenuSceneDraw {
        crate::menu_rendering::draw_menu_native_components_with_progress_texture(
            &self.menu,
            draw_flags,
            resolve_progress_texture,
            resources,
            text_precision,
        )
    }

    #[cfg(test)]
    pub(crate) fn menu_identity_for_test(
        &self,
    ) -> crate::menu_native_drawing::MenuNativeMenuIdentity {
        self.menu.identity()
    }

    #[cfg(test)]
    pub(crate) fn menu_base_for_test(
        &self,
    ) -> (
        Option<crate::menu_native_drawing::MenuNativeMenuIdentity>,
        u32,
    ) {
        (self.menu.parent(), self.menu.flags())
    }

    #[cfg(test)]
    pub(crate) fn with_components_for_test<R>(
        &self,
        operation: impl FnOnce(&crate::menu::MenuStack) -> Result<R, String>,
    ) -> Result<R, crate::menu_native_drawing::MenuNativeMenuAccessError> {
        self.menu.with_components(operation)
    }

    pub fn destroy(&mut self, inputs: &mut dyn LoadProgressCreateInputs) -> Result<(), String> {
        if let Some(instance) = &self.shared_instance {
            inputs
                .shared_lifecycle()
                .ok_or("loading-menu shared owner unavailable")?
                .check_shared_instance(instance)?;
        }
        self.destroy_components(inputs)?;
        if let Some(instance) = &mut self.shared_instance {
            inputs
                .shared_lifecycle()
                .ok_or("loading-menu shared owner unavailable")?
                .destroy_shared(instance)?;
        }
        Ok(())
    }

    fn destroy_components(&self, inputs: &mut dyn LoadProgressCreateInputs) -> Result<(), String> {
        self.menu
            .with_components_mut(|menu| {
                if menu.has_native_lifecycle() {
                    let mut resources = LoadProgressComponentResources {
                        inputs,
                        unavailable:
                            "native loading-menu resource owner is unavailable for destruction",
                    };
                    menu.destroy_native(&mut resources)
                } else {
                    menu.destroy();
                    Ok(())
                }
            })
            .map_err(|error| error.to_string())
    }

    fn set_text_id(
        &mut self,
        id: u32,
        text_id: u32,
        inputs: &mut dyn LoadProgressCreateInputs,
    ) -> Result<(), String> {
        self.menu
            .with_components_mut(|menu| {
                if menu.has_native_lifecycle() {
                    let mut resources = LoadProgressComponentResources {
                        inputs,
                        unavailable: "native loading-menu text owner is unavailable",
                    };
                    menu.set_text_id_native(id, text_id, &mut resources)
                } else {
                    menu.set_text_id(id, text_id);
                    Ok(())
                }
            })
            .map_err(|error| error.to_string())
    }

    fn set_string(
        &mut self,
        id: u32,
        string: Option<&[u8]>,
        inputs: &mut dyn LoadProgressCreateInputs,
    ) -> Result<(), String> {
        self.menu
            .with_components_mut(|menu| {
                if menu.has_native_lifecycle() {
                    let mut resources = LoadProgressComponentResources {
                        inputs,
                        unavailable: "native loading-menu texture owner is unavailable",
                    };
                    menu.set_string_native(id, string, &mut resources)
                } else {
                    menu.set_string(id, string);
                    Ok(())
                }
            })
            .map_err(|error| error.to_string())
    }

    /// The inline `SetRawProgress` (`SetFloat(LOADPROGRESS_BAR, progress)`,
    /// g_m_func.h:247; no outline body).
    pub fn set_raw_progress(&mut self, progress: f32) -> Result<(), String> {
        self.menu
            .with_components_mut(|menu| menu.set_float(crate::menu::LOADPROGRESS_BAR_ID, progress))
            .map_err(|error| error.to_string())
    }

    /// `GM_LoadProgress::SetProgressGameInit` (GAME 0x5e3cf0): the credited
    /// [`crate::menu::load_progress_game_init_value`] math over the live
    /// `mission_part`, then the inline `SetRawProgress`.
    pub fn set_progress_game_init(&mut self, progress: f32) -> Result<(), String> {
        let raw = crate::menu::load_progress_game_init_value(self.mission_part, progress);
        self.set_raw_progress(raw)
    }

    /// `GM_LoadProgress::SetProgressMissionOpen` (GAME 0x5e3d20, REA
    /// ev_93ac45a5be: `fld [esp+4]; fmul [ecx+0x30]; fstp float`, then
    /// `SetFloat(0x12c, ..)` at 0x5e3d30): the credited
    /// [`crate::menu::load_progress_mission_open_value`] over the live
    /// `mission_part`, then the bar write.
    pub fn set_progress_mission_open(&mut self, progress: f32) -> Result<(), String> {
        let raw = crate::menu::load_progress_mission_open_value(self.mission_part, progress);
        self.set_raw_progress(raw)
    }

    /// `GM_Menu::Create` (GAME 0x5ea2e0, g_menu.cpp:610-625) for this class:
    /// `components.clear()`, `CreateCompsByDef(OnBeforeCreate())` over
    /// `"loadprogress"` (GAME 0x5e3b80), `ActivateByIDOrNext(1)`, then
    /// [`LoadProgressScreen::on_create`] and the shared open sound. The
    /// parentless screen's `OnChange` is the base `return 0`. A failed
    /// operation stops with the components built so far.
    pub fn create(&mut self, inputs: &mut dyn LoadProgressCreateInputs) -> Result<(), String> {
        if let Some(instance) = &self.shared_instance {
            inputs
                .shared_lifecycle()
                .ok_or("loading-menu shared owner unavailable")?
                .check_shared(instance)?;
            self.menu
                .with_components_mut(crate::menu::MenuStack::prepare_empty_loading_recreate)
                .map_err(|error| error.to_string())?;
        } else {
            self.destroy_components(inputs)?;
        }
        let name = crate::main_menu::load_progress_definition();
        let definition = inputs.menu_definition(name)?;
        let native_layout = inputs.native_layout();
        if self.has_shared_instance() && native_layout.is_none() {
            return Err("loading-menu layout unavailable".into());
        }
        if let Some(context) = native_layout {
            if let Some(definition) = definition {
                let mut resources = LoadProgressComponentResources {
                    inputs,
                    unavailable: "native loading-menu creation requires its resource owner",
                };
                self.menu
                    .with_components_mut(|menu| {
                        menu.create_components_native(&definition, context, &mut resources)
                    })
                    .map_err(|error| error.to_string())?;
            } else {
                self.menu
                    .with_components_mut(|menu| menu.create_empty_native())
                    .map_err(|error| error.to_string())?;
            }
        } else if let Some(definition) = definition {
            self.menu
                .with_components_mut(|menu| {
                    *menu = crate::menu::MenuStack::create_from_definition(&definition);
                    Ok(())
                })
                .map_err(|error| error.to_string())?;
        }
        self.menu
            .with_components_mut(|menu| {
                menu.activate_by_id_or_next(1);
                Ok(())
            })
            .map_err(|error| error.to_string())?;
        self.on_create(inputs)?;
        if self.has_shared_instance() {
            let closing = self
                .menu
                .with_components(|menu| Ok(menu.is_closing()))
                .map_err(|error| error.to_string())?;
            inputs
                .shared_lifecycle()
                .ok_or("loading-menu shared owner unavailable")?
                .play_open_sound(closing)?;
            if self.menu.parent().is_some() {
                return Err("loading-menu parent OnCreateChild unavailable".into());
            }
        }
        Ok(())
    }

    /// `GM_LoadProgress::OnCreate` (GAME 0x5e3b90, g_m_func.cpp:1017-1058).
    /// Game.exe SHA-256 08398317…16ab27, Ghidra 12.1.4 body
    /// 0x5e3b90..0x5e3cc7 (REA ev_e5206d5bb7). In order: with a title,
    /// `SetTextID(0x190, title)` and `SetTextID(0x191, 0x2ce)`;
    /// `SetString(0x64, "loadbok.tga")`, `SetString(0x66, "mafia.tga")`;
    /// `tables\load.def` opened (a failed open skips to the cleanup), read
    /// up to 8000 bytes, `read > 0` gives `read / 0x44` entries; the first
    /// entry whose `mission` (+0) `_stricmp`-equals the screen's
    /// `mission_name`, else `act_mission->mission_name` (reread per entry),
    /// gets `SetString(0x65, entry+0x20)` and, for a nonzero +0x40,
    /// `SetTextID(0xc8, name_id)`, and the scan ends; finally
    /// `SetHidden(1000/2000/3000, false)`. Returns 0.
    pub fn on_create(&mut self, inputs: &mut dyn LoadProgressCreateInputs) -> Result<(), String> {
        if let Some(texts) = crate::main_menu::load_progress_title_texts(self.title) {
            for (id, text_id) in texts {
                self.set_text_id(id, text_id, inputs)?;
            }
        }
        self.set_string(LOADPROGRESS_SIDE, Some(b"loadbok.tga"), inputs)?;
        self.set_string(LOADPROGRESS_LOGO, Some(b"mafia.tga"), inputs)?;
        if let Some(mut bytes) = inputs.load_def()? {
            bytes.truncate(LOAD_DEF_BUFFER);
            let count = bytes.len() / LOAD_DEF_ENTRY;
            for entry in 0..count {
                let base = entry * LOAD_DEF_ENTRY;
                let name = match &self.mission_name {
                    Some(name) => name.clone(),
                    None => inputs.act_mission_name()?,
                };
                if stricmp_eq(&bytes, base, &name)? {
                    let picture = c_string_at(&bytes, base + 0x20)?.to_vec();
                    self.set_string(LOADPROGRESS_MISSION_PICTURE, Some(&picture), inputs)?;
                    let name_id =
                        u32::from_le_bytes(bytes[base + 0x40..base + 0x44].try_into().unwrap());
                    if name_id != 0 {
                        self.set_text_id(LOADPROGRESS_MISSION_NAME, name_id, inputs)?;
                    }
                    break;
                }
            }
        }
        for id in [
            LOADPROGRESS_UNKNOWN_1000,
            LOADPROGRESS_UNKNOWN_2000,
            LOADPROGRESS_UNKNOWN_3000,
        ] {
            // SetHidden(id, false) (`push 0` at 0x5e3c98/0x5e3ca4/0x5e3cb2).
            self.menu
                .with_components_mut(|menu| {
                    menu.set_hidden(id, false);
                    Ok(())
                })
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

/// The `freeride` global (`S_freeride`, PS2 name; GAME 0x6d4540, mafia-re
/// `game/ui/g_loadsave.cpp:96-97`). Field spellings are upstream working
/// names. This is one semantic owner, not a native memory-layout mirror.
/// Its native process-global lifetime is not composed: the session currently
/// owns an instance, and production menu/profile borrowing remains unavailable.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FreerideState {
    pub car: u32,
    pub flags: u32,
    pub location: u32,
    pub traffic_density: f32,
    pub pedestrian_density: f32,
    pub police_density: f32,
    /// +0x18: a free ride is on (mafia-re's guessed name).
    pub active: bool,
}

impl FreerideState {
    /// FreerideStoreA, GAME RVA 0x20e920: preserve location and active.
    pub fn store_a(&mut self, car: u32, flags: u32, traffic: f32, pedestrians: f32, police: f32) {
        self.car = car;
        self.flags = flags;
        self.traffic_density = traffic;
        self.pedestrian_density = pedestrians;
        self.police_density = police;
    }

    /// FreerideStoreB, GAME RVA 0x20e960: one complete unsigned word store.
    /// REA ev_319f9fb5a16ae0a88154ba940daf0c65e1825fde4172b0eeb5998fe54ece6e63,
    /// same GAME hash as SaveGameNewGame below. StoreA's REA body is
    /// ev_0c5d349fdc3e562637469f885db6f855a2271a939e6f5133d413f9cf800eff2b.
    pub fn store_b(&mut self, location: u32) {
        self.location = location;
    }
}

/// FUNCTION: GAME RVA 0x205ff0, VA 0x605ff0 (mafia-re `fe7e746`,
/// `game/ui/g_loadsave.cpp:470-477`, `G_LoadSaveClass::SaveGameNewGame`).
/// Game.exe SHA-256 08398317…16ab27, Ghidra 12.1.4 body 0x605ff0..0x606010
/// (33 bytes, 5 instructions; REA ev_dfd059fc0b): `mov [ecx+0x1e8],0`,
/// `mov [ecx+0x98],0`, `call 0x60b940` (GarageClear, same `this`),
/// `mov byte [0x6d4558],0`, `ret`. Direct callers 0x5bebe0 (Main's
/// new-game cases) and 0x6051e0 (`AllDefault`; REA ev_fffaa6495d). The
/// stores run in that order and nothing is read.
pub fn save_game_new_game(progress: &mut LoadSaveProgress, freeride: &mut FreerideState) {
    progress.last_stolen = None;
    progress.steal_cars = 0;
    garage_clear(&mut progress.garage);
    freeride.active = false;
}

#[cfg(test)]
mod loadsave_gates_tests {
    use super::*;

    #[test]
    fn freeride_stores_and_new_game_preserve_unrelated_words_and_float_bits() {
        let mut state = FreerideState {
            location: 7,
            active: true,
            ..Default::default()
        };
        state.store_a(
            0xffab_0001,
            0x8000_0100,
            f32::from_bits(0x7fc1_2345),
            -0.0,
            f32::INFINITY,
        );
        assert_eq!(state.location, 7);
        assert!(state.active);
        state.store_b(u32::MAX);
        save_game_new_game(&mut LoadSaveProgress::default(), &mut state);
        assert!(!state.active);
        assert_eq!(state.car, 0xffab_0001);
        assert_eq!(state.flags, 0x8000_0100);
        assert_eq!(state.location, u32::MAX);
        assert_eq!(state.traffic_density.to_bits(), 0x7fc1_2345);
        assert_eq!(state.pedestrian_density.to_bits(), 0x8000_0000);
        assert_eq!(state.police_density.to_bits(), 0x7f80_0000);
    }

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
    fn save_game_new_game_rva_205ff0_clears_progress_and_freeride_active() {
        let mut progress = LoadSaveProgress {
            steal_cars: 0b101 | (1 << 25),
            last_stolen: Some(7),
            ..Default::default()
        };
        progress.garage.slots[3] = GarageSlot {
            car: 0x1234,
            actor: Some(7),
        };
        progress.garage.shown_cars[24] = 0x55;
        let mut freeride = FreerideState {
            active: true,
            ..Default::default()
        };
        save_game_new_game(&mut progress, &mut freeride);
        assert_eq!(progress, LoadSaveProgress::default());
        assert_eq!(freeride, FreerideState::default());
        // Idempotent: a second new game over the cleared state is a no-op.
        save_game_new_game(&mut progress, &mut freeride);
        assert_eq!(progress, LoadSaveProgress::default());
        assert!(!freeride.active);
    }

    #[test]
    fn save_game_new_game_keeps_the_loading_screen() {
        // SaveGameNewGame's five instructions never touch +0x5d4.
        let mut progress = LoadSaveProgress {
            progress: Some(LoadProgressScreen::for_mission_open()),
            ..Default::default()
        };
        let identity = progress.progress.as_ref().unwrap().menu.identity();
        save_game_new_game(&mut progress, &mut FreerideState::default());
        let screen = progress.progress.as_ref().unwrap();
        assert_eq!(screen.menu.identity(), identity);
        assert_eq!(screen.title, 0);
        assert_eq!(screen.mission_part.to_bits(), 0.7f32.to_bits());
        assert_eq!(screen.mission_name, None);
        assert!(!screen.uses_native_components().unwrap());
        screen
            .menu
            .with_components(|menu| {
                assert!(menu.is_empty());
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn profile_intake_takes_all_default_options_and_refuses_a_profile_file() {
        // AllDefault -> DefaultSettings(0xffff): `mov [edx+0x51c], bl` (bl = 1).
        assert_eq!(
            profile_settings_after_load_save_init(ProfileFileIntake::NoProfileFile),
            Ok(ProfileSettings {
                speedometer_mph: true
            })
        );
        // No default stands in for an existing profile's G_Stream bytes.
        assert!(
            profile_settings_after_load_save_init(ProfileFileIntake::ProfileFilePresent)
                .unwrap_err()
                .contains("ProfileLoad (GAME 0x6057f0)")
        );
        // SaveGameNewGame leaves the options alone.
        let mut progress = LoadSaveProgress {
            settings: Some(ProfileSettings {
                speedometer_mph: true,
            }),
            ..Default::default()
        };
        save_game_new_game(&mut progress, &mut FreerideState::default());
        assert_eq!(
            progress.settings,
            Some(ProfileSettings {
                speedometer_mph: true
            })
        );
    }

    #[test]
    fn mission_open_loading_screen_carries_the_open_constructor_arguments() {
        // GAME 0x540a9d-0x540aa6 pushes NULL, 0x3f333333 and 0 for
        // GM_LoadProgress(title, mission_part, mission_name).
        let screen = LoadProgressScreen::for_mission_open();
        assert_eq!(screen.title, 0);
        assert_eq!(screen.mission_part.to_bits(), 0x3f33_3333);
        assert_eq!(screen.mission_name, None);
    }

    #[test]
    fn bar_writes_need_the_created_bar_component() {
        let mut screen = LoadProgressScreen::for_mission_open();
        // Before Create the GM_Menu base has no component: SetFloat is the
        // native no-op and the bar reads the base default.
        screen.set_progress_game_init(0.0).unwrap();
        assert_eq!(screen.raw_progress().unwrap(), 0.0);
        assert!(screen
            .menu
            .with_components(|menu| Ok(menu.is_empty()))
            .unwrap());
        let definition = lhr_assets::MenuDefinition {
            version: 1,
            reserved: 0,
            items: vec![lhr_assets::MenuItem {
                id: crate::menu::LOADPROGRESS_BAR_ID,
                kind: *b"rabp",
                rect: [0.0, 0.0, 1.0, 1.0],
                text_id: 0,
                style: 0,
            }],
        };
        screen
            .menu
            .with_components_mut(|menu| {
                *menu = crate::menu::MenuStack::create_from_definition(&definition);
                Ok(())
            })
            .unwrap();
        screen.set_progress_game_init(0.0).unwrap();
        assert_eq!(screen.raw_progress().unwrap().to_bits(), 0.7f32.to_bits());
        // The live mission_part is read, not the Open constant.
        screen.mission_part = 0.5;
        screen.set_progress_game_init(0.5).unwrap();
        assert_eq!(screen.raw_progress().unwrap(), 0.75);
        screen.set_progress_mission_open(0.5).unwrap();
        assert_eq!(screen.raw_progress().unwrap(), 0.25);
    }

    struct NativeProgressInputs {
        definition: lhr_assets::MenuDefinition,
        definition_missing: bool,
        resources_available: bool,
        resource_requests: usize,
        precision: crate::menu::MenuComponentPrecision,
        definition_reads: usize,
        load_def_reads: usize,
        load_def_error: bool,
        text_reads: Vec<u32>,
        text_error: bool,
        textures: Option<crate::itexture::TextureResources>,
        released_textures: Vec<crate::itexture::TextureKey>,
    }

    impl NativeProgressInputs {
        fn new() -> Self {
            Self {
                definition: lhr_assets::MenuDefinition {
                    version: 1,
                    reserved: 0,
                    items: vec![
                        lhr_assets::MenuItem {
                            id: 1,
                            kind: *b"cniw",
                            rect: [0.0, 0.0, 800.0, 600.0],
                            text_id: 0,
                            style: crate::main_menu::COMPONENT_VISIBLE,
                        },
                        lhr_assets::MenuItem {
                            id: crate::menu::LOADPROGRESS_BAR_ID,
                            kind: *b"rabp",
                            rect: [20.0, 570.0, 200.0, 8.0],
                            text_id: 0,
                            style: crate::main_menu::COMPONENT_VISIBLE,
                        },
                    ],
                },
                definition_missing: false,
                resources_available: true,
                resource_requests: 0,
                precision: crate::menu::MenuComponentPrecision::Pc24Nearest,
                definition_reads: 0,
                load_def_reads: 0,
                load_def_error: false,
                text_reads: Vec::new(),
                text_error: false,
                textures: None,
                released_textures: Vec::new(),
            }
        }
    }

    impl crate::menu_draw_resources::MenuComponentResources for NativeProgressInputs {
        fn text(&mut self, text_id: u32) -> Result<Option<std::sync::Arc<[u8]>>, String> {
            self.text_reads.push(text_id);
            if self.text_error {
                Err("fixture text lookup interrupted".to_owned())
            } else {
                Ok(Some(std::sync::Arc::from(b"fixture".as_slice())))
            }
        }

        fn create_alpha_texture(
            &mut self,
            _: &[u8],
        ) -> Result<Option<crate::itexture::TextureKey>, String> {
            self.textures
                .as_mut()
                .ok_or_else(|| "unexpected fixture texture creation".to_owned())?
                .create_texture()
                .map(Some)
                .map_err(|error| format!("{error:?}"))
        }

        fn release_texture(&mut self, key: crate::itexture::TextureKey) -> Result<(), String> {
            self.released_textures.push(key);
            Err("unexpected fixture texture release".to_owned())
        }
    }

    impl LoadProgressCreateInputs for NativeProgressInputs {
        fn native_layout(&self) -> Option<crate::menu::MenuComponentCreateContext> {
            Some(crate::menu::MenuComponentCreateContext {
                screen_scale: [1.0, 1.0],
                left_margin_x: 0.0,
                precision: self.precision,
            })
        }

        fn component_resources(
            &mut self,
        ) -> Option<&mut dyn crate::menu_draw_resources::MenuComponentResources> {
            self.resource_requests += 1;
            if self.resources_available {
                Some(self)
            } else {
                None
            }
        }

        fn menu_definition(
            &mut self,
            name: &str,
        ) -> Result<Option<lhr_assets::MenuDefinition>, String> {
            assert_eq!(name, "loadprogress");
            self.definition_reads += 1;
            Ok((!self.definition_missing).then(|| self.definition.clone()))
        }

        fn load_def(&mut self) -> Result<Option<Vec<u8>>, String> {
            self.load_def_reads += 1;
            if self.load_def_error {
                Err("fixture load.def read interrupted".to_owned())
            } else {
                Ok(None)
            }
        }

        fn act_mission_name(&mut self) -> Result<Vec<u8>, String> {
            Err("unexpected fixture mission-name read".to_owned())
        }
    }

    #[test]
    fn loading_menu_missing_definition_creates_an_authoritative_empty_native_owner() {
        let mut screen = LoadProgressScreen::new(42, 0.7, None);
        let identity = screen.menu.identity();
        let mut inputs = NativeProgressInputs::new();
        inputs.definition_missing = true;
        inputs.resources_available = false;
        inputs.precision = crate::menu::MenuComponentPrecision::Unestablished;
        screen.create(&mut inputs).unwrap();
        assert!(screen.uses_native_components().unwrap());
        assert_eq!(screen.menu.identity(), identity);
        screen
            .menu
            .with_components(|menu| {
                assert!(menu.has_native_lifecycle());
                assert!(menu.is_empty());
                assert_eq!(menu.native_root_key()?, None);
                Ok(())
            })
            .unwrap();
        screen.set_raw_progress(0.5).unwrap();
        assert_eq!(screen.raw_progress().unwrap(), 0.0);
        assert_eq!(inputs.definition_reads, 1);
        assert_eq!(inputs.load_def_reads, 1);
        assert!(inputs.text_reads.is_empty());
        screen.destroy(&mut inputs).unwrap();
        assert!(!screen.uses_native_components().unwrap());
        assert_eq!(screen.menu.identity(), identity);
        screen
            .menu
            .with_components(|menu| {
                assert!(!menu.has_native_lifecycle());
                assert!(menu.is_empty());
                Ok(())
            })
            .unwrap();
        screen.create(&mut inputs).unwrap();
        assert!(screen.uses_native_components().unwrap());
        assert_eq!(screen.menu.identity(), identity);
        assert_eq!(inputs.definition_reads, 2);
        assert_eq!(inputs.load_def_reads, 2);
        assert!(inputs.text_reads.is_empty());
        assert_eq!(inputs.resource_requests, 0);
    }

    #[test]
    fn loading_menu_missing_definition_retains_the_empty_owner_on_on_create_error() {
        let mut screen = LoadProgressScreen::new(42, 0.7, None);
        let identity = screen.menu.identity();
        let mut inputs = NativeProgressInputs::new();
        inputs.definition_missing = true;
        inputs.resources_available = false;
        inputs.precision = crate::menu::MenuComponentPrecision::Unestablished;
        inputs.load_def_error = true;
        assert_eq!(
            screen.create(&mut inputs).unwrap_err(),
            "fixture load.def read interrupted"
        );
        assert!(screen.uses_native_components().unwrap());
        assert_eq!(screen.menu.identity(), identity);
        screen
            .menu
            .with_components(|menu| {
                assert!(menu.has_native_lifecycle());
                assert!(menu.is_empty());
                assert_eq!(menu.native_root_key()?, None);
                Ok(())
            })
            .unwrap();
        assert_eq!(inputs.definition_reads, 1);
        assert_eq!(inputs.load_def_reads, 1);
        assert!(inputs.text_reads.is_empty());
        screen.destroy(&mut inputs).unwrap();
        assert!(!screen.uses_native_components().unwrap());
        assert_eq!(screen.menu.identity(), identity);
        assert_eq!(inputs.load_def_reads, 1);
        assert!(inputs.text_reads.is_empty());
    }

    #[test]
    fn loading_menu_missing_resource_owner_stops_at_the_reached_lookup() {
        let mut screen = LoadProgressScreen::for_mission_open();
        let identity = screen.menu.identity();
        let mut inputs = NativeProgressInputs::new();
        inputs.resources_available = false;
        inputs.definition.items[0].text_id = 42;
        assert_eq!(
            screen.create(&mut inputs).unwrap_err(),
            "native loading-menu creation requires its resource owner"
        );
        assert!(screen.uses_native_components().unwrap());
        assert_eq!(screen.menu.identity(), identity);
        screen
            .menu
            .with_components(|menu| {
                assert!(menu.has_native_lifecycle());
                assert_eq!(menu.len(), 1);
                assert_eq!(menu.native_root_key()?, None);
                assert_eq!(menu.find_component_by_id(1).unwrap().text_id, 42);
                Ok(())
            })
            .unwrap();
        assert_eq!(inputs.definition_reads, 1);
        assert_eq!(inputs.load_def_reads, 0);
        assert!(inputs.text_reads.is_empty());
        assert_eq!(inputs.resource_requests, 1);
        assert_eq!(
            screen.on_create(&mut inputs).unwrap_err(),
            "native menu component construction/destruction is incomplete"
        );
        assert_eq!(inputs.resource_requests, 1);
        screen.destroy(&mut inputs).unwrap();
        assert!(!screen.uses_native_components().unwrap());
        assert_eq!(inputs.resource_requests, 1);
        inputs.resources_available = true;
        screen.create(&mut inputs).unwrap();
        assert!(screen.uses_native_components().unwrap());
        assert_eq!(screen.menu.identity(), identity);
        assert_eq!(inputs.definition_reads, 2);
        assert_eq!(inputs.load_def_reads, 1);
        assert_eq!(inputs.resource_requests, 2);
        assert_eq!(inputs.text_reads, [42]);
    }

    #[test]
    fn loading_menu_textless_components_need_no_resource_owner() {
        let mut screen = LoadProgressScreen::new(42, 0.7, None);
        let identity = screen.menu.identity();
        let mut inputs = NativeProgressInputs::new();
        inputs.resources_available = false;
        screen.create(&mut inputs).unwrap();
        assert!(screen.uses_native_components().unwrap());
        assert_eq!(screen.menu.identity(), identity);
        assert_eq!(inputs.definition_reads, 1);
        assert_eq!(inputs.load_def_reads, 1);
        screen.set_text_id(1, 0, &mut inputs).unwrap();
        screen.set_text_id(999, 42, &mut inputs).unwrap();
        screen.set_string(1, None, &mut inputs).unwrap();
        screen.set_raw_progress(0.375).unwrap();
        assert_eq!(screen.raw_progress().unwrap(), 0.375);
        assert_eq!(inputs.resource_requests, 0);
        assert!(inputs.text_reads.is_empty());
        assert_eq!(
            screen.set_text_id(1, 42, &mut inputs).unwrap_err(),
            "native loading-menu text owner is unavailable"
        );
        screen
            .menu
            .with_components(|menu| {
                assert_eq!(menu.len(), 2);
                assert_eq!(menu.find_component_by_id(1).unwrap().text_id, 42);
                assert!(menu.native_root_key()?.is_some());
                Ok(())
            })
            .unwrap();
        assert_eq!(inputs.resource_requests, 1);
        screen.destroy(&mut inputs).unwrap();
        assert!(!screen.uses_native_components().unwrap());
        assert_eq!(screen.menu.identity(), identity);
        assert_eq!(inputs.resource_requests, 1);
        assert!(inputs.text_reads.is_empty());
    }

    #[test]
    fn loading_menu_numeric_create_error_retains_the_empty_interrupted_native_owner() {
        let mut screen = LoadProgressScreen::for_mission_open();
        let identity = screen.menu.identity();
        let mut inputs = NativeProgressInputs::new();
        inputs.resources_available = false;
        inputs.precision = crate::menu::MenuComponentPrecision::Unestablished;
        assert_eq!(
            screen.create(&mut inputs).unwrap_err(),
            "unsupported unestablished native component precision at multiply"
        );
        assert!(screen.uses_native_components().unwrap());
        assert_eq!(screen.menu.identity(), identity);
        screen
            .menu
            .with_components(|menu| {
                assert!(menu.has_native_lifecycle());
                assert!(menu.is_empty());
                assert_eq!(menu.native_root_key()?, None);
                Ok(())
            })
            .unwrap();
        assert_eq!(inputs.definition_reads, 1);
        assert_eq!(inputs.load_def_reads, 0);
        assert!(inputs.text_reads.is_empty());
        screen.destroy(&mut inputs).unwrap();
        assert!(!screen.uses_native_components().unwrap());
        assert_eq!(screen.menu.identity(), identity);
        assert_eq!(inputs.resource_requests, 0);
    }

    #[test]
    fn loading_menu_identity_survives_native_create_destroy_and_recreate() {
        let mut screen = LoadProgressScreen::for_mission_open();
        let alias = screen.menu.clone();
        let identity = alias.identity();
        let mut inputs = NativeProgressInputs::new();
        screen.create(&mut inputs).unwrap();
        assert_eq!(screen.menu.identity(), identity);
        assert!(screen.uses_native_components().unwrap());
        let first_root = alias
            .with_components(|menu| menu.native_root_key())
            .unwrap()
            .unwrap();
        screen.set_raw_progress(0.375).unwrap();
        alias
            .with_components(|menu| {
                assert_eq!(
                    menu.progress_value(crate::menu::LOADPROGRESS_BAR_ID),
                    Some(0.375)
                );
                assert_eq!(menu.get_float(crate::menu::LOADPROGRESS_BAR_ID), 0.0);
                Ok(())
            })
            .unwrap();
        screen.destroy(&mut inputs).unwrap();
        assert_eq!(screen.menu.identity(), identity);
        assert!(!screen.uses_native_components().unwrap());
        alias
            .with_components(|menu| {
                assert!(menu.is_empty());
                assert_eq!(menu.native_root_key()?, None);
                Ok(())
            })
            .unwrap();
        screen.create(&mut inputs).unwrap();
        let second_root = alias
            .with_components(|menu| menu.native_root_key())
            .unwrap()
            .unwrap();
        assert_ne!(first_root, second_root);
        assert_eq!(screen.menu.identity(), identity);
        assert_eq!(screen.raw_progress().unwrap(), 0.0);
        assert_eq!(inputs.definition_reads, 2);
        assert_eq!(inputs.load_def_reads, 2);
    }

    #[test]
    fn loading_menu_alias_destroy_allows_screen_cleanup_and_recreate() {
        let mut screen = LoadProgressScreen::for_mission_open();
        let alias = screen.menu.clone();
        let identity = alias.identity();
        let mut inputs = NativeProgressInputs::new();
        screen.create(&mut inputs).unwrap();
        let first_root = alias
            .with_components(|menu| menu.native_root_key())
            .unwrap()
            .unwrap();
        alias
            .with_components_mut(|menu| menu.destroy_native(&mut inputs))
            .unwrap();
        assert!(!screen.uses_native_components().unwrap());
        assert_eq!(screen.menu.identity(), identity);
        screen.destroy(&mut inputs).unwrap();
        screen.destroy(&mut inputs).unwrap();
        assert!(!screen.uses_native_components().unwrap());
        assert!(alias.with_components(|menu| Ok(menu.is_empty())).unwrap());
        screen.create(&mut inputs).unwrap();
        let second_root = alias
            .with_components(|menu| menu.native_root_key())
            .unwrap()
            .unwrap();
        assert_ne!(first_root, second_root);
        assert!(screen.uses_native_components().unwrap());
        assert_eq!(screen.menu.identity(), identity);
        assert_eq!(screen.raw_progress().unwrap(), 0.0);
        assert_eq!(inputs.definition_reads, 2);
        assert_eq!(inputs.load_def_reads, 2);
        assert_eq!(inputs.resource_requests, 0);
    }

    #[test]
    fn loading_menu_setters_follow_an_alias_installed_native_lifecycle() {
        let mut screen = LoadProgressScreen::for_mission_open();
        let alias = screen.menu.clone();
        let identity = alias.identity();
        let mut inputs = NativeProgressInputs::new();
        inputs.definition.items.push(lhr_assets::MenuItem {
            id: LOADPROGRESS_SIDE,
            kind: *b"ttcr",
            rect: [0.0, 0.0, 30.0, 40.0],
            text_id: 0,
            style: crate::main_menu::COMPONENT_VISIBLE,
        });
        let definition = inputs.definition.clone();
        let context = inputs.native_layout().unwrap();
        alias
            .with_components_mut(|menu| {
                menu.create_components_native(&definition, context, &mut inputs)
            })
            .unwrap();
        assert!(screen.uses_native_components().unwrap());
        screen.set_text_id(1, 654, &mut inputs).unwrap();
        assert_eq!(inputs.text_reads, vec![654]);
        inputs.resources_available = false;
        assert_eq!(
            screen
                .set_string(LOADPROGRESS_SIDE, Some(b"alias.tga"), &mut inputs)
                .unwrap_err(),
            "native loading-menu texture owner is unavailable"
        );
        alias
            .with_components(|menu| {
                assert_eq!(menu.find_component_by_id(1).unwrap().text_id, 654);
                assert_eq!(menu.staged_string(LOADPROGRESS_SIDE), None);
                assert_eq!(
                    menu.find_component_by_id(LOADPROGRESS_SIDE)
                        .unwrap()
                        .texture_key()?,
                    None
                );
                Ok(())
            })
            .unwrap();
        assert_eq!(inputs.resource_requests, 2);
        screen.destroy(&mut inputs).unwrap();
        assert!(!screen.uses_native_components().unwrap());
        assert_eq!(screen.menu.identity(), identity);
        assert_eq!(inputs.resource_requests, 2);
        assert_eq!(inputs.definition_reads, 0);
        assert_eq!(inputs.load_def_reads, 0);
    }

    #[test]
    fn loading_menu_setters_and_cleanup_follow_an_alias_installed_legacy_stack() {
        let mut screen = LoadProgressScreen::for_mission_open();
        let alias = screen.menu.clone();
        let identity = alias.identity();
        let mut inputs = NativeProgressInputs::new();
        screen.create(&mut inputs).unwrap();
        alias
            .with_components_mut(|menu| {
                *menu = crate::menu::MenuStack::create_from_definition(&inputs.definition);
                Ok(())
            })
            .unwrap();
        inputs.resources_available = false;
        assert!(!screen.uses_native_components().unwrap());
        screen.set_text_id(1, 655, &mut inputs).unwrap();
        screen
            .set_string(1, Some(b"legacy.tga"), &mut inputs)
            .unwrap();
        alias
            .with_components(|menu| {
                assert!(!menu.has_native_lifecycle());
                assert_eq!(menu.find_component_by_id(1).unwrap().text_id, 655);
                assert_eq!(menu.staged_string(1), Some(Some(b"legacy.tga".as_slice())));
                Ok(())
            })
            .unwrap();
        screen.destroy(&mut inputs).unwrap();
        assert!(!screen.uses_native_components().unwrap());
        assert!(alias.with_components(|menu| Ok(menu.is_empty())).unwrap());
        assert_eq!(screen.menu.identity(), identity);
        assert_eq!(inputs.resource_requests, 0);
        assert!(inputs.text_reads.is_empty());
    }

    #[test]
    fn loading_menu_alias_interrupted_destroy_remains_native_without_release_replay() {
        let mut screen = LoadProgressScreen::for_mission_open();
        let alias = screen.menu.clone();
        let identity = alias.identity();
        let mut inputs = NativeProgressInputs::new();
        inputs.definition.items.push(lhr_assets::MenuItem {
            id: LOADPROGRESS_SIDE,
            kind: *b"ttcr",
            rect: [0.0, 0.0, 30.0, 40.0],
            text_id: 0,
            style: crate::main_menu::COMPONENT_VISIBLE,
        });
        inputs.textures = Some(crate::itexture::TextureResources::new().unwrap());
        screen.create(&mut inputs).unwrap();
        let texture = alias
            .with_components(|menu| {
                Ok(*menu
                    .find_component_by_id(LOADPROGRESS_SIDE)
                    .unwrap()
                    .texture_key()?
                    .unwrap())
            })
            .unwrap();
        assert_eq!(
            alias
                .with_components_mut(|menu| menu.destroy_native(&mut inputs))
                .unwrap_err()
                .to_string(),
            "unexpected fixture texture release"
        );
        assert!(screen.uses_native_components().unwrap());
        assert_eq!(inputs.released_textures, vec![texture]);
        inputs.resources_available = false;
        for error in [
            screen.destroy(&mut inputs).unwrap_err(),
            screen.create(&mut inputs).unwrap_err(),
        ] {
            assert_eq!(
                error,
                "native texture Release is interrupted; backend reconciliation is required, not replay"
            );
        }
        for error in [
            screen.set_text_id(1, 656, &mut inputs).unwrap_err(),
            screen
                .set_string(LOADPROGRESS_SIDE, Some(b"late.tga"), &mut inputs)
                .unwrap_err(),
            screen.set_raw_progress(0.5).unwrap_err(),
        ] {
            assert_eq!(
                error,
                "native menu component construction/destruction is incomplete"
            );
        }
        assert!(screen.uses_native_components().unwrap());
        alias
            .with_components(|menu| {
                assert!(menu.has_native_lifecycle());
                assert_eq!(
                    menu.find_component_by_id(LOADPROGRESS_SIDE)
                        .unwrap()
                        .interrupted_texture_key(),
                    Some(&texture)
                );
                Ok(())
            })
            .unwrap();
        assert_eq!(screen.menu.identity(), identity);
        assert_eq!(inputs.released_textures, vec![texture]);
        assert_eq!(inputs.definition_reads, 1);
        assert_eq!(inputs.load_def_reads, 1);
        assert_eq!(inputs.resource_requests, 1);
        assert!(inputs.text_reads.is_empty());
    }

    #[test]
    fn loading_menu_native_lifecycle_query_reports_an_aliased_borrow_conflict() {
        let mut screen = LoadProgressScreen::for_mission_open();
        let alias = screen.menu.clone();
        let mut inputs = NativeProgressInputs::new();
        screen.create(&mut inputs).unwrap();
        alias
            .with_components_mut(|menu| {
                assert!(menu.has_native_lifecycle());
                assert_eq!(
                    screen.uses_native_components().unwrap_err(),
                    "native menu components are borrowed"
                );
                assert_eq!(
                    screen.destroy(&mut inputs).unwrap_err(),
                    "native menu components are borrowed"
                );
                assert!(menu.has_native_lifecycle());
                Ok(())
            })
            .unwrap();
        assert!(screen.uses_native_components().unwrap());
        assert_eq!(inputs.resource_requests, 0);
        screen.destroy(&mut inputs).unwrap();
        assert!(!screen.uses_native_components().unwrap());
    }

    #[test]
    fn loading_menu_partial_create_preserves_the_allocation_and_root_null_prefix() {
        let mut screen = LoadProgressScreen::for_mission_open();
        let alias = screen.menu.clone();
        let identity = alias.identity();
        let mut inputs = NativeProgressInputs::new();
        inputs.definition.items[0].text_id = 650;
        inputs.text_error = true;
        assert_eq!(
            screen.create(&mut inputs).unwrap_err(),
            "fixture text lookup interrupted"
        );
        assert_eq!(inputs.text_reads, vec![650]);
        assert_eq!(inputs.load_def_reads, 0);
        assert!(screen.uses_native_components().unwrap());
        assert_eq!(screen.menu.identity(), identity);
        alias
            .with_components(|menu| {
                assert_eq!(menu.len(), 1);
                assert_eq!(menu.native_root_key()?, None);
                Ok(())
            })
            .unwrap();
        screen.destroy(&mut inputs).unwrap();
        assert!(!screen.uses_native_components().unwrap());
        assert_eq!(screen.menu.identity(), identity);
        assert_eq!(inputs.text_reads, vec![650]);
        assert!(alias.with_components(|menu| Ok(menu.is_empty())).unwrap());
    }

    #[test]
    fn loading_menu_partial_progress_create_returns_error_without_scalar_write() {
        let mut screen = LoadProgressScreen::for_mission_open();
        let alias = screen.menu.clone();
        let identity = alias.identity();
        let mut inputs = NativeProgressInputs::new();
        inputs.definition.items[1].text_id = 651;
        inputs.text_error = true;
        assert_eq!(
            screen.create(&mut inputs).unwrap_err(),
            "fixture text lookup interrupted"
        );
        let root = alias
            .with_components(|menu| {
                assert_eq!(menu.len(), 2);
                assert_eq!(
                    menu.find_component_by_id(crate::menu::LOADPROGRESS_BAR_ID)
                        .unwrap()
                        .text_id,
                    651
                );
                menu.native_root_key()
            })
            .unwrap();
        assert!(root.is_some());
        assert_eq!(
            screen.set_raw_progress(0.75).unwrap_err(),
            "native menu component construction/destruction is incomplete"
        );
        assert_eq!(screen.raw_progress().unwrap(), 0.0);
        assert_eq!(screen.menu.identity(), identity);
        assert!(screen.uses_native_components().unwrap());
        assert_eq!(inputs.text_reads, vec![651]);
        assert_eq!(inputs.load_def_reads, 0);
        alias
            .with_components(|menu| {
                assert_eq!(menu.len(), 2);
                assert_eq!(menu.native_root_key()?, root);
                assert_eq!(menu.get_float(crate::menu::LOADPROGRESS_BAR_ID), 0.0);
                Ok(())
            })
            .unwrap();
        screen.destroy(&mut inputs).unwrap();
        assert!(!screen.uses_native_components().unwrap());
        assert_eq!(screen.menu.identity(), identity);
        assert_eq!(inputs.text_reads, vec![651]);
        assert!(alias.with_components(|menu| Ok(menu.is_empty())).unwrap());
    }

    #[test]
    fn loading_menu_borrow_failure_preserves_progress_and_stops_before_intake() {
        let mut screen = LoadProgressScreen::for_mission_open();
        let alias = screen.menu.clone();
        let mut inputs = NativeProgressInputs::new();
        screen.create(&mut inputs).unwrap();
        screen.set_raw_progress(0.625).unwrap();
        alias
            .with_components_mut(|menu| {
                assert_eq!(
                    screen.raw_progress().unwrap_err(),
                    "native menu components are borrowed"
                );
                assert_eq!(
                    screen.set_raw_progress(0.25).unwrap_err(),
                    "native menu components are borrowed"
                );
                assert_eq!(
                    screen.create(&mut inputs).unwrap_err(),
                    "native menu components are borrowed"
                );
                assert_eq!(
                    screen.destroy(&mut inputs).unwrap_err(),
                    "native menu components are borrowed"
                );
                assert!(menu.has_native_lifecycle());
                assert_eq!(
                    screen.uses_native_components().unwrap_err(),
                    "native menu components are borrowed"
                );
                assert_eq!(
                    menu.progress_value(crate::menu::LOADPROGRESS_BAR_ID),
                    Some(0.625)
                );
                assert_eq!(inputs.definition_reads, 1);
                assert_eq!(inputs.load_def_reads, 1);
                Ok(())
            })
            .unwrap();
        assert_eq!(screen.raw_progress().unwrap(), 0.625);
        screen.destroy(&mut inputs).unwrap();
    }

    #[test]
    fn load_save_progress_default_is_the_zeroed_global() {
        let progress = LoadSaveProgress::default();
        assert_eq!(progress.steal_cars, 0);
        assert_eq!(progress.last_stolen, None);
        assert_eq!(progress.progress, None);
        assert_eq!(progress.garage, LoadSaveGarage::new());
        assert!(!FreerideState::default().active);
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
