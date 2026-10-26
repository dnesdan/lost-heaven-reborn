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
pub trait LoadSaveEffects {
    type Error;
    fn all_default(&mut self, profile: &mut ProfileState) -> Result<(), Self::Error>;
    fn create_indicators(&mut self) -> Result<(), Self::Error>;
    fn resolution_changed(&mut self) -> Result<(), Self::Error>;
    fn clear_indicator_slot_40a4(&mut self) -> Result<(), Self::Error>;
    fn enumerate_profiles(&mut self, profile: &mut ProfileState) -> Result<(), Self::Error>;
    fn null_timer(&mut self) -> Result<(), Self::Error>;
    fn open_menu_mission(&mut self) -> Result<(), Self::Error>;
    fn execute_profile_select(&mut self) -> Result<i32, Self::Error>;
    fn profile_load(&mut self, profile: &mut ProfileState) -> Result<(), Self::Error>;
    fn profile_save(&mut self, profile: &ProfileState) -> Result<(), Self::Error>;
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
