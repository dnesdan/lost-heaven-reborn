//! Game startup/shutdown caller reconstruction. Names of platform operations are
//! descriptive. Backend calls are real dependency boundaries, not implementations
//! of graphics, profiles, input, mission loading or Windows desktop services.
//! Game SHA256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27;
//! InitSystem RVA 0x1bf5a0, CloseSystem RVA 0x1c0080.

use crate::original_config::OriginalConfig;

pub use lhr_assets::startup_archives::{ARCHIVE_NAMES, ARCHIVE_UNLOCK_KEYS};

/// Stable, process-unique identity of one persistent dependency context. Mint
/// once per backend owner and return the same token after moves and reentry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SystemDomainId(u64);
impl Default for SystemDomainId {
    fn default() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(
            NEXT.try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .expect("system dependency identity exhausted"),
        )
    }
}
fn bind_domain(
    bound: &mut Option<SystemDomainId>,
    incoming: SystemDomainId,
) -> Result<(), SystemError> {
    if let Some(previous) = *bound {
        if previous != incoming {
            return Err(SystemError::ForeignDependencyDomain);
        }
    } else {
        *bound = Some(incoming);
    }
    Ok(())
}

/// Opaque identity supplied by the owning dependency. This is not a native
/// pointer and never creates ownership by itself. Backends must reject foreign IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SystemHandle(pub u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SystemError {
    ForeignDependencyDomain,
    Unsupported(&'static str),
    Backend(&'static str),
    InvalidReply(&'static str),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphInitDescriptor {
    pub adapter: u32,
    pub instance: SystemHandle,
    pub rect: [i32; 4],
    pub child_rect: [i32; 4],
    pub flags: u32,
    pub bit_depth: u8,
    pub refresh_rate: u8,
    pub menu: Option<SystemHandle>,
    pub error_handler: GraphErrorCallback,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphErrorCallback {
    OriginalRva1beb00,
}
impl GraphInitDescriptor {
    pub fn from_config(config: &OriginalConfig, instance: SystemHandle) -> Self {
        let b = config.as_bytes();
        let flag = |offset: usize, mask| if b[offset] != 0 { mask } else { 0 };
        Self {
            adapter: config.adapter_index() as i8 as i32 as u32,
            instance,
            rect: [50, 50, config.width() as i32, config.height() as i32],
            child_rect: [0, 0, config.width() as i32, config.height() as i32],
            flags: 0x10090
                | flag(19, 0x4000)
                | if b[18] == 4 { 0x2000 } else { 0 }
                | if b[18] == 2 { 0x1000 } else { 0 }
                | flag(23, 0x100)
                | flag(31, 0x20)
                | flag(25, 4)
                | flag(21, 2)
                | flag(22, 0x10)
                | flag(20, 0x8000),
            bit_depth: config.color_depth() as u8,
            refresh_rate: config.refresh_rate() as u8,
            menu: None,
            error_handler: GraphErrorCallback::OriginalRva1beb00,
        }
    }
}

/// Operations with no returned native value used by these callers. Each variant
/// is a dependency contract; successful return requires its actual effect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SystemEffect {
    DtaFirstForce,
    CloseFile(SystemHandle),
    ReleaseArchive(SystemHandle),
    UnlockArchive(SystemHandle, u64),
    MissingArchiveMessage(usize),
    VersionMismatchMessage([u8; 2], [u8; 2]),
    GraphNotFoundMessage,
    EngineErrorMessage {
        result: i32,
        base: i32,
    },
    GraphSetAppName(SystemHandle, Vec<u8>),
    GraphResetRenderProps(SystemHandle),
    GraphClear(SystemHandle),
    GraphPresent(SystemHandle),
    SetWindowIcon {
        instance: SystemHandle,
        graph: SystemHandle,
    },
    DriverSetState(SystemHandle, u32, u32),
    GraphSetAlphaRef(SystemHandle, u32),
    MissionInit(SystemHandle),
    SceneSetCallback,
    KeyboardInit(SystemHandle, u32),
    MouseInit(SystemHandle, u32),
    ShowCursor(bool),
    InitJoy {
        instance: SystemHandle,
        graph: SystemHandle,
    },
    ControlsInit(SystemHandle),
    ForceFeedbackReset,
    AddMapsDir(SystemHandle, &'static [u8], bool),
    AddCacheDir(CacheKind, &'static [u8]),
    ConsolesInit,
    LoadTyres,
    LoadVehicleMaterials,
    LoadParticles,
    LoadPatchFile(&'static [u8]),
    InitAnimIndex,
    LoadSoundTables(&'static [u8]),
    LanguageSelect(u32),
    ProfileClose,
    DoneAnimIndex,
    CloseTyres,
    CloseVehicleMaterials,
    CloseParticles,
    ConsolesDone,
    MissionUninit,
    ClearCache(CacheKind),
    ClearAnimationTracks,
    UnloadPatchFiles,
    DriverClose(SystemHandle),
    ForceFeedbackClose,
    CloseJoy,
    SoundClose(SystemHandle),
    GraphClose(SystemHandle),
    FreeSoundTables,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheKind {
    Model,
    Sound,
    Animation,
}

/// Calls distinguish native failure values from unavailable host capabilities.
/// An unavailable call returns Err and leaves all already executed effects in
/// place; the caller never turns it into native success or automatic rollback.
/// Reuse requires the same persistent dependency context, including unkeyed global
/// subsystems. SystemState binds its stable token before any effect. Each backend
/// owner must mint one token and retain it across moves and calls; manufacturing
/// a new token per call or reusing another owner's token violates this contract.
pub trait SystemBackend {
    fn domain_id(&self) -> SystemDomainId;
    fn effect(&mut self, effect: SystemEffect) -> Result<(), SystemError>;
    fn create_archive(&mut self, path: &[u8]) -> Result<Option<SystemHandle>, SystemError>;
    fn find_game_file(&mut self, name: &[u8]) -> Result<Option<Vec<u8>>, SystemError>;
    fn open_file(&mut self, path: &[u8]) -> Result<Option<SystemHandle>, SystemError>;
    /// Return at most count bytes, including an empty read on native failure.
    fn read_file(&mut self, file: SystemHandle, count: usize) -> Result<Vec<u8>, SystemError>;
    fn get_graph(&mut self) -> Result<Option<SystemHandle>, SystemError>;
    fn graph_init(
        &mut self,
        graph: SystemHandle,
        desc: &GraphInitDescriptor,
    ) -> Result<i32, SystemError>;
    fn load_resource_string(
        &mut self,
        instance: SystemHandle,
        id: u32,
        limit: usize,
    ) -> Result<Vec<u8>, SystemError>;
    fn get_sound_driver(&mut self) -> Result<SystemHandle, SystemError>;
    fn sound_init(
        &mut self,
        sound: SystemHandle,
        graph: SystemHandle,
        flags: u32,
    ) -> Result<i32, SystemError>;
    fn get_driver(&mut self) -> Result<SystemHandle, SystemError>;
    fn driver_init(&mut self, driver: SystemHandle, flags: u32) -> Result<i32, SystemError>;
    fn play_logo(&mut self, name: &[u8], size: [f32; 2]) -> Result<bool, SystemError>;
    fn profile_init(&mut self) -> Result<bool, SystemError>;
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct SystemState {
    domain: Option<SystemDomainId>,
    pub archives: [Option<SystemHandle>; 14],
    pub graph: Option<SystemHandle>,
    pub sound: Option<SystemHandle>,
    pub driver: Option<SystemHandle>,
}

impl SystemState {
    /// Native false is a completed caller return and does not write `fatal`.
    /// Only the final profile call assigns fatal, even when it reports failure.
    /// Reentry deliberately overwrites globals without releasing former owners:
    /// their owning backend must keep those actual external survivors visible.
    pub fn init_system(
        &mut self,
        backend: &mut impl SystemBackend,
        config: &OriginalConfig,
        instance: SystemHandle,
        fatal: &mut u8,
    ) -> Result<bool, SystemError> {
        bind_domain(&mut self.domain, backend.domain_id())?;
        use SystemEffect as E;
        backend.effect(E::DtaFirstForce)?;
        for (slot, name) in self.archives.iter_mut().zip(ARCHIVE_NAMES) {
            *slot = backend.create_archive(name)?;
        }
        if self.archives[5].is_none() {
            if let Some(path) = backend.find_game_file(b"a1.dta")? {
                self.archives[5] = backend.create_archive(&path)?;
            }
        }
        for index in (0..14).rev() {
            if index != 1 && self.archives[index].is_none() {
                backend.effect(E::MissingArchiveMessage(index))?;
                return Ok(false);
            }
        }
        for (slot, key) in self.archives.into_iter().zip(ARCHIVE_UNLOCK_KEYS) {
            if let Some(handle) = slot {
                backend.effect(E::UnlockArchive(handle, key))?;
            }
        }
        let version = read_version(backend, b"tables\\version.txt")?;
        let patch = read_version(backend, b"patch\\version.txt")?;
        if version != patch {
            self.release_archives(backend)?;
            backend.effect(E::VersionMismatchMessage(version, patch))?;
            return Ok(false);
        }
        let desc = GraphInitDescriptor::from_config(config, instance);
        self.graph = backend.get_graph()?;
        let Some(graph) = self.graph else {
            backend.effect(E::GraphNotFoundMessage)?;
            return Ok(false);
        };
        let result = backend.graph_init(graph, &desc)?;
        if result != 0 {
            backend.effect(E::EngineErrorMessage { result, base: 4200 })?;
            return Ok(false);
        }
        let name = backend.load_resource_string(instance, 3, 127)?;
        if name.len() > 127 {
            return Err(SystemError::InvalidReply(
                "resource string exceeds native buffer",
            ));
        }
        backend.effect(E::GraphSetAppName(graph, name))?;
        backend.effect(E::GraphResetRenderProps(graph))?;
        for _ in 0..3 {
            backend.effect(E::GraphClear(graph))?;
            backend.effect(E::GraphPresent(graph))?;
        }
        backend.effect(E::SetWindowIcon { instance, graph })?;
        let b = config.as_bytes();
        if b[29] == 0 {
            let sound = backend.get_sound_driver()?;
            self.sound = Some(sound);
            let speakers = match b[33] as i8 {
                0 => 0x400,
                1 => 0x800,
                2 => 0x1000,
                3 => 0x2000,
                _ => 0,
            };
            let flags = 2
                | speakers
                | if b[30] != 0 { 0x10 } else { 0 }
                | if b[32] != 0 { 0x20 } else { 0 };
            let result = backend.sound_init(sound, graph, flags)?;
            if result < 0 {
                backend.effect(E::EngineErrorMessage { result, base: 8810 })?;
                // No Release/Close: the original simply loses this global pointer.
                self.sound = None;
            }
        }
        let driver = backend.get_driver()?;
        self.driver = Some(driver);
        let result = backend.driver_init(driver, 0)?;
        if result < 0 {
            backend.effect(E::EngineErrorMessage { result, base: 5630 })?;
            return Ok(false);
        }
        for (state, value) in [
            (49, 1),
            (13, 1),
            (19, 1),
            (54, 8),
            (55, 8),
            (43, 1),
            (44, 0),
            (45, 0),
            (46, 0),
            (47, 0),
            (39, 1),
            (17, u32::from(b[28])),
            (21, u32::from(b[24])),
            (25, u32::from(b[27])),
            (34, u32::from(b[26])),
            (2, 2),
        ] {
            backend.effect(E::DriverSetState(driver, state, value))?;
        }
        backend.effect(E::GraphSetAlphaRef(graph, 200))?;
        backend.effect(E::DriverSetState(driver, 21, u32::from(b[24])))?;
        for effect in [
            E::MissionInit(driver),
            E::SceneSetCallback,
            E::KeyboardInit(graph, 1),
            E::MouseInit(graph, 1),
            E::ShowCursor(false),
            E::InitJoy { instance, graph },
            E::ControlsInit(graph),
            E::ForceFeedbackReset,
            E::AddMapsDir(graph, b"Maps", false),
            E::AddCacheDir(CacheKind::Model, b"Models"),
            E::AddCacheDir(CacheKind::Sound, b"Sounds"),
            E::AddCacheDir(CacheKind::Animation, b"Anims"),
            E::ConsolesInit,
            E::LoadTyres,
            E::LoadVehicleMaterials,
            E::LoadParticles,
            E::LoadPatchFile(b"patch\\a1.bin"),
            E::LoadPatchFile(b"patch\\a2.bin"),
            E::LoadPatchFile(b"patch\\a3.bin"),
            E::LoadPatchFile(b"patch\\a4.bin"),
            E::InitAnimIndex,
            E::LoadSoundTables(b"tables\\sounds.dat"),
        ] {
            backend.effect(effect)?;
        }
        if let Some(file) = backend.open_file(b"tables\\version.txt")? {
            let mut bytes = backend.read_file(file, 2)?;
            let language = match bytes.len() {
                2 => {
                    for byte in &mut bytes {
                        if byte.is_ascii_uppercase() {
                            *byte = byte.wrapping_sub(0x20);
                        }
                    }
                    u32::from(bytes[1]) | (u32::from(bytes[0]) << 8)
                }
                0 | 1 => 0x656e,
                _ => return Err(SystemError::InvalidReply("version read exceeds request")),
            };
            backend.effect(E::LanguageSelect(language))?;
            backend.effect(E::CloseFile(file))?;
        } else {
            backend.effect(E::LanguageSelect(0x656e))?;
        }
        backend.play_logo(b"logo1", [800.0, 600.0])?;
        backend.play_logo(b"logo3", [800.0, 600.0])?;
        backend.play_logo(b"logo2", [704.0, 400.0])?;
        *fatal = u8::from(!backend.profile_init()?);
        Ok(true)
    }

    fn release_archives(&mut self, backend: &mut impl SystemBackend) -> Result<(), SystemError> {
        for slot in &mut self.archives {
            if let Some(handle) = *slot {
                backend.effect(SystemEffect::ReleaseArchive(handle))?;
                *slot = None;
            }
        }
        Ok(())
    }

    /// No native initialization guard exists: unconditional subsystem teardown
    /// is called even on a partial start. A host capability error stops exactly
    /// there; later pointers and archives remain live for explicit recovery.
    pub fn close_system(&mut self, backend: &mut impl SystemBackend) -> Result<(), SystemError> {
        bind_domain(&mut self.domain, backend.domain_id())?;
        use SystemEffect as E;
        for effect in [
            E::ProfileClose,
            E::DoneAnimIndex,
            E::CloseTyres,
            E::CloseVehicleMaterials,
            E::CloseParticles,
            E::ConsolesDone,
            E::MissionUninit,
            E::ClearCache(CacheKind::Model),
            E::ClearCache(CacheKind::Sound),
            E::ClearCache(CacheKind::Animation),
            E::ClearAnimationTracks,
            E::UnloadPatchFiles,
        ] {
            backend.effect(effect)?;
        }
        if let Some(driver) = self.driver {
            backend.effect(E::DriverClose(driver))?;
            self.driver = None;
        }
        backend.effect(E::ForceFeedbackClose)?;
        backend.effect(E::CloseJoy)?;
        if let Some(sound) = self.sound {
            backend.effect(E::SoundClose(sound))?;
            self.sound = None;
        }
        if let Some(graph) = self.graph {
            backend.effect(E::GraphClose(graph))?;
            self.graph = None;
        }
        self.release_archives(backend)?;
        backend.effect(E::FreeSoundTables)
    }
}

fn read_version(backend: &mut impl SystemBackend, path: &[u8]) -> Result<[u8; 2], SystemError> {
    let mut version = [b'?'; 2];
    if let Some(file) = backend.open_file(path)? {
        let bytes = backend.read_file(file, 2)?;
        if bytes.len() > 2 {
            return Err(SystemError::InvalidReply("version read exceeds request"));
        }
        version[..bytes.len()].copy_from_slice(&bytes);
        backend.effect(SystemEffect::CloseFile(file))?;
    }
    Ok(version)
}

/// Descriptive formatting projection of InitErrorMessageBox (RVA0x1bfec0).
/// The unknown branch intentionally preserves the original missing percent sign.
pub fn init_error_text(result: i32, base: i32) -> String {
    let texts = [
        "No file...",
        "Out of memory...",
        "Bad texture format...",
        "Bad texture...",
        "Generic...",
        "Object not found...",
        "Invalid param...",
        "Not loaded...",
        "Not initialized...",
        "File corupted...",
        "No file 1...",
        "Cyclic link...",
        "Canceled...",
        "Unsupported...",
        "No 3D hardware...",
        "Buffer lost...",
        "User break...",
        "File system error...",
        "Bad file version...",
        "Bad file time...",
        "Out of limit...",
    ];
    let index = (result as u32).wrapping_sub(0x80000001);
    if let Some(text) = texts.get(index as usize) {
        let code = (result & 0xffff).wrapping_add(base);
        format!("error: #{code}\r\n{text}")
    } else {
        "error: 0x08X\r\nUnknown graph init error...".into()
    }
}

/// Desktop message dependency shared by the recovered error-message wrapper.
/// Bytes exclude the terminating NUL; actual native MessageBox flags and result
/// are preserved. Missing desktop capability must return Unsupported.
pub trait MessageBoxBackend {
    fn message_box(
        &mut self,
        window: Option<SystemHandle>,
        text: &[u8],
        caption: Option<&[u8]>,
        flags: u32,
    ) -> Result<i32, SystemError>;
}
/// InitErrorMessageBox RVA0x1bfec0. The MessageBox result is ignored natively.
pub fn init_error_message_box(
    backend: &mut impl MessageBoxBackend,
    result: i32,
    base: i32,
) -> Result<(), SystemError> {
    let text = init_error_text(result, base);
    backend.message_box(None, text.as_bytes(), Some(b"Init error"), 0x10)?;
    Ok(())
}

/// PC-only InitJoy/CloseJoy caller projections (RVAs0x1bd5f0/0x1bd870).
/// Device APIs retain ownership; the pointer vector owns their returned creator
/// references. Native vector storage remains allocated after clear. Implementations
/// must reject unknown/foreign IDs and preserve the same dependency context across
/// init/close; handles do not authorize another backend's device ownership.
pub trait JoystickBackend {
    fn domain_id(&self) -> SystemDomainId;
    fn create_base(
        &mut self,
        instance: SystemHandle,
        graph: SystemHandle,
    ) -> Result<(bool, Option<SystemHandle>), SystemError>;
    fn num_devices(&mut self, base: SystemHandle) -> Result<i32, SystemError>;
    fn create_joy(
        &mut self,
        base: SystemHandle,
        index: i32,
        flags: u32,
    ) -> Result<(bool, Option<SystemHandle>), SystemError>;
    fn set_axis_mode(&mut self, joy: SystemHandle, axis: u32, mode: u32)
        -> Result<(), SystemError>;
    fn send_force_feedback(&mut self, joy: SystemHandle, command: u32) -> Result<(), SystemError>;
    fn release_joy(&mut self, joy: SystemHandle) -> Result<(), SystemError>;
    fn release_base(&mut self, base: SystemHandle) -> Result<(), SystemError>;
}
#[derive(Debug, Default, PartialEq, Eq)]
pub struct JoystickState {
    domain: Option<SystemDomainId>,
    pub base: Option<SystemHandle>,
    slots: Vec<Option<SystemHandle>>,
    count: usize,
}
impl JoystickState {
    pub fn live_devices(&self) -> impl Iterator<Item = SystemHandle> + '_ {
        self.slots[..self.count]
            .iter()
            .map(|slot| slot.expect("live joystick slot"))
    }
    pub fn count(&self) -> usize {
        self.count
    }
    /// Native vector capacity; actual backing slots remain until this owner dies.
    pub fn capacity(&self) -> usize {
        self.slots.len()
    }
    pub fn init_joy(
        &mut self,
        backend: &mut impl JoystickBackend,
        instance: SystemHandle,
        graph: SystemHandle,
    ) -> Result<(), SystemError> {
        bind_domain(&mut self.domain, backend.domain_id())?;
        let (success, base) = backend.create_base(instance, graph)?;
        self.base = base;
        if !success {
            return Ok(());
        }
        let Some(base) = base else {
            return Err(SystemError::Unsupported(
                "InitJoy successful null base native dereference",
            ));
        };
        let devices = backend.num_devices(base)?;
        for index in 0..devices {
            let (success, joy) = backend.create_joy(base, index, 1)?;
            let Some(joy) = joy else {
                return Err(SystemError::Unsupported(
                    "InitJoy null joystick native dereference",
                ));
            };
            if !success {
                backend.set_axis_mode(joy, 1, 2)?;
            }
            if self.count == self.slots.len() {
                let increment = self.count.max(1);
                let new_capacity =
                    self.count
                        .checked_add(increment)
                        .ok_or(SystemError::Unsupported(
                            "InitJoy native vector allocation domain",
                        ))?;
                let mut slots = Vec::new();
                slots
                    .try_reserve_exact(new_capacity)
                    .map_err(|_| SystemError::Backend("InitJoy vector allocation"))?;
                slots.resize(new_capacity, None);
                slots[..self.count].copy_from_slice(&self.slots[..self.count]);
                self.slots = slots;
            }
            self.slots[self.count] = Some(joy);
            self.count += 1;
            for command in [3, 6, 4, 5] {
                backend.send_force_feedback(joy, command)?;
            }
        }
        Ok(())
    }
    pub fn close_joy(&mut self, backend: &mut impl JoystickBackend) -> Result<(), SystemError> {
        bind_domain(&mut self.domain, backend.domain_id())?;
        for index in 0..self.count {
            backend.release_joy(self.slots[index].expect("live joystick slot"))?;
        }
        self.count = 0;
        if let Some(base) = self.base {
            backend.release_base(base)?;
            self.base = None;
        }
        Ok(())
    }
}

/// PC-only GraphErrorHandler (descriptive name), RVA0x1beb00. The process-exit
/// dependency is nonreturning. A host without settings/desktop capability returns
/// Unsupported at that exact boundary; it must not pretend the process exited.
pub trait GraphErrorBackend: MessageBoxBackend {
    fn save_settings(&mut self, config: &OriginalConfig) -> Result<(), SystemError>;
    fn clear_graph(&mut self, graph: SystemHandle) -> Result<(), SystemError>;
    fn present_graph(&mut self, graph: SystemHandle) -> Result<(), SystemError>;
    fn graph_window(&mut self, graph: SystemHandle) -> Result<Option<SystemHandle>, SystemError>;
    fn close_graph(&mut self, graph: SystemHandle) -> Result<(), SystemError>;
    fn exit_process(&mut self, code: i32) -> Result<std::convert::Infallible, SystemError>;
}
pub fn graph_error_handler(
    result: i32,
    config: &mut OriginalConfig,
    graph: Option<SystemHandle>,
    backend: &mut impl GraphErrorBackend,
) -> Result<(), SystemError> {
    if result as u32 != 0x80000002 {
        return Ok(());
    }
    let mut bytes = config.clone().into_bytes();
    bytes[28] = 0;
    *config = OriginalConfig::from_bytes(&bytes).expect("same packed config size");
    backend.save_settings(config)?;
    let Some(graph) = graph else {
        return Err(SystemError::Unsupported(
            "GraphErrorHandler null graph native dereference",
        ));
    };
    backend.clear_graph(graph)?;
    backend.present_graph(graph)?;
    let window = backend.graph_window(graph)?;
    backend.message_box(
        window,
        b"error: #8888\r\nVideo memory overflow...",
        None,
        0x10,
    )?;
    backend.close_graph(graph)?;
    match backend.exit_process(0)? {}
}

/// Borrowed projection of an already existing force-feedback record. Reset and
/// Close are descriptive working names, not recovered PC symbols. This view does
/// not construct the object or initialize any unrelated bytes.
pub struct ForceFeedbackManagerRecord<'a> {
    bytes: &'a mut [u8],
}
impl<'a> ForceFeedbackManagerRecord<'a> {
    pub fn from_record(bytes: &'a mut [u8]) -> Result<Self, SystemError> {
        if bytes.len() < 0x24 {
            return Err(SystemError::Unsupported("force-feedback record prefix"));
        }
        Ok(Self { bytes })
    }
    /// Game RVA0x446a0: exactly MOV dword ptr [ECX+0x20],0; RET.
    pub fn reset(&mut self) {
        self.bytes[0x20..0x24].fill(0);
    }
    /// Game RVA0xaabe0: the folded one-byte RET, with no receiver access.
    pub fn close(&mut self) {}
    pub fn word_20(&self) -> u32 {
        u32::from_le_bytes(self.bytes[0x20..0x24].try_into().unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct ArchiveFixture {
        domain: SystemDomainId,
        missing: Vec<usize>,
        next: usize,
        fallback: bool,
        versions: [Option<Vec<u8>>; 3],
        opened: usize,
        effects: Vec<SystemEffect>,
        creates: Vec<Vec<u8>>,
        graph_error: bool,
    }
    impl SystemBackend for ArchiveFixture {
        fn domain_id(&self) -> SystemDomainId {
            self.domain
        }
        fn effect(&mut self, e: SystemEffect) -> Result<(), SystemError> {
            self.effects.push(e);
            Ok(())
        }
        fn create_archive(&mut self, path: &[u8]) -> Result<Option<SystemHandle>, SystemError> {
            self.creates.push(path.to_vec());
            let index = self.next;
            self.next += 1;
            Ok((!self.missing.contains(&index)).then_some(SystemHandle(100 + index as u64)))
        }
        fn find_game_file(&mut self, _: &[u8]) -> Result<Option<Vec<u8>>, SystemError> {
            Ok(self.fallback.then(|| b"C:\\MafiaGame\\a1.dta".to_vec()))
        }
        fn open_file(&mut self, _: &[u8]) -> Result<Option<SystemHandle>, SystemError> {
            let i = self.opened;
            self.opened += 1;
            Ok(self.versions[i].as_ref().map(|_| SystemHandle(i as u64)))
        }
        fn read_file(&mut self, file: SystemHandle, _: usize) -> Result<Vec<u8>, SystemError> {
            Ok(self.versions[file.0 as usize].clone().unwrap())
        }
        fn get_graph(&mut self) -> Result<Option<SystemHandle>, SystemError> {
            if self.graph_error {
                Err(SystemError::Unsupported("IGraph DLL loader"))
            } else {
                Ok(None)
            }
        }
        fn graph_init(
            &mut self,
            _: SystemHandle,
            _: &GraphInitDescriptor,
        ) -> Result<i32, SystemError> {
            Err(SystemError::Unsupported("graph init"))
        }
        fn load_resource_string(
            &mut self,
            _: SystemHandle,
            _: u32,
            _: usize,
        ) -> Result<Vec<u8>, SystemError> {
            Err(SystemError::Unsupported("resource"))
        }
        fn get_sound_driver(&mut self) -> Result<SystemHandle, SystemError> {
            Err(SystemError::Unsupported("sound"))
        }
        fn sound_init(
            &mut self,
            _: SystemHandle,
            _: SystemHandle,
            _: u32,
        ) -> Result<i32, SystemError> {
            Err(SystemError::Unsupported("sound"))
        }
        fn get_driver(&mut self) -> Result<SystemHandle, SystemError> {
            Err(SystemError::Unsupported("driver"))
        }
        fn driver_init(&mut self, _: SystemHandle, _: u32) -> Result<i32, SystemError> {
            Err(SystemError::Unsupported("driver"))
        }
        fn play_logo(&mut self, _: &[u8], _: [f32; 2]) -> Result<bool, SystemError> {
            Err(SystemError::Unsupported("logo"))
        }
        fn profile_init(&mut self) -> Result<bool, SystemError> {
            Err(SystemError::Unsupported("profiles"))
        }
    }
    fn init(
        f: &mut ArchiveFixture,
        state: &mut SystemState,
        fatal: &mut u8,
    ) -> Result<bool, SystemError> {
        state.init_system(f, &OriginalConfig::default(), SystemHandle(1), fatal)
    }
    #[test]
    fn error_message_wrapper_uses_native_caption_flags_and_ignores_cancel() {
        struct Desktop {
            calls: usize,
        }
        impl MessageBoxBackend for Desktop {
            fn message_box(
                &mut self,
                window: Option<SystemHandle>,
                text: &[u8],
                caption: Option<&[u8]>,
                flags: u32,
            ) -> Result<i32, SystemError> {
                assert_eq!(window, None);
                assert_eq!(caption, Some(b"Init error".as_slice()));
                assert_eq!(flags, 0x10);
                assert_eq!(text, b"error: #5651\r\nOut of limit...");
                self.calls += 1;
                Ok(2)
            }
        }
        let mut desktop = Desktop { calls: 0 };
        assert_eq!(
            init_error_message_box(&mut desktop, 0x80000015u32 as i32, 5630),
            Ok(())
        );
        assert_eq!(desktop.calls, 1);
    }
    #[test]
    fn persistent_system_domain_rejects_foreign_init_and_close_before_any_effect() {
        let mut a = ArchiveFixture {
            graph_error: true,
            ..Default::default()
        };
        let mut b = ArchiveFixture::default();
        let mut state = SystemState::default();
        let mut fatal = 0xa5;
        assert_eq!(
            init(&mut a, &mut state, &mut fatal),
            Err(SystemError::Unsupported("IGraph DLL loader"))
        );
        let handles = state.archives;
        let domain = state.domain;
        assert_ne!(a.domain_id(), b.domain_id());
        assert_eq!(
            state.close_system(&mut b),
            Err(SystemError::ForeignDependencyDomain)
        );
        assert_eq!(
            init(&mut b, &mut state, &mut fatal),
            Err(SystemError::ForeignDependencyDomain)
        );
        assert!(b.effects.is_empty());
        assert!(b.creates.is_empty());
        assert_eq!(b.opened, 0);
        assert_eq!(state.archives, handles);
        assert_eq!(state.domain, domain);
        assert_eq!(fatal, 0xa5);
        state.close_system(&mut a).unwrap();
        assert_eq!(state.archives, [None; 14]);
        assert_eq!(state.domain, domain);
        assert_eq!(
            state.close_system(&mut b),
            Err(SystemError::ForeignDependencyDomain)
        );
        assert!(b.effects.is_empty());
    }
    #[test]
    fn force_feedback_leaves_preserve_every_unrelated_record_byte() {
        let mut bytes = [0xa5; 0xbc];
        {
            let mut record = ForceFeedbackManagerRecord::from_record(&mut bytes).unwrap();
            record.close();
            assert_eq!(record.word_20(), 0xa5a5a5a5);
            record.reset();
            assert_eq!(record.word_20(), 0);
            record.close();
        }
        let mut expected = [0xa5; 0xbc];
        expected[0x20..0x24].fill(0);
        assert_eq!(bytes, expected);
        assert!(matches!(
            ForceFeedbackManagerRecord::from_record(&mut bytes[..0x23]),
            Err(SystemError::Unsupported("force-feedback record prefix"))
        ));
    }
    #[test]
    fn required_archives_fail_in_reverse_order_without_unlock_or_cleanup() {
        let mut f = ArchiveFixture {
            missing: vec![0, 13],
            ..Default::default()
        };
        let mut s = SystemState::default();
        let mut fatal = 0xa5;
        assert_eq!(init(&mut f, &mut s, &mut fatal), Ok(false));
        assert_eq!(fatal, 0xa5);
        assert_eq!(f.creates, ARCHIVE_NAMES.map(Vec::from));
        assert_eq!(
            f.effects,
            vec![
                SystemEffect::DtaFirstForce,
                SystemEffect::MissingArchiveMessage(13)
            ]
        );
        assert_eq!(s.archives.iter().flatten().count(), 12);
    }
    #[test]
    fn optional_archive_and_a1_fallback_preserve_actual_slots() {
        let mut f = ArchiveFixture {
            missing: vec![1, 5],
            fallback: true,
            ..Default::default()
        };
        let mut s = SystemState::default();
        let mut fatal = 0xa5;
        assert_eq!(init(&mut f, &mut s, &mut fatal), Ok(false));
        assert_eq!(s.archives[1], None);
        assert_eq!(s.archives[5], Some(SystemHandle(114)));
        assert_eq!(f.creates.len(), 15);
        assert_eq!(fatal, 0xa5);
        assert_eq!(
            f.effects
                .iter()
                .filter(|e| matches!(e, SystemEffect::UnlockArchive(..)))
                .count(),
            13
        );
        assert_eq!(f.effects.last(), Some(&SystemEffect::GraphNotFoundMessage));
    }
    #[test]
    fn mismatch_releases_ascending_and_reentry_never_releases_overwritten_globals() {
        let mut f = ArchiveFixture {
            versions: [Some(b"en".to_vec()), Some(b"cz".to_vec()), None],
            ..Default::default()
        };
        let mut s = SystemState {
            archives: [Some(SystemHandle(999)); 14],
            ..Default::default()
        };
        let mut fatal = 0xa5;
        assert_eq!(init(&mut f, &mut s, &mut fatal), Ok(false));
        assert_eq!(s.archives, [None; 14]);
        assert_eq!(fatal, 0xa5);
        let released: Vec<_> = f
            .effects
            .iter()
            .filter_map(|e| {
                if let SystemEffect::ReleaseArchive(h) = e {
                    Some(h.0)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(released, (100..114).collect::<Vec<_>>());
        assert_eq!(
            f.effects.last(),
            Some(&SystemEffect::VersionMismatchMessage(*b"en", *b"cz"))
        );
    }
    #[test]
    fn unavailable_graph_keeps_acquired_archive_owners_and_fatal_byte() {
        let mut f = ArchiveFixture {
            graph_error: true,
            ..Default::default()
        };
        let mut s = SystemState::default();
        let mut fatal = 0xa5;
        assert_eq!(
            init(&mut f, &mut s, &mut fatal),
            Err(SystemError::Unsupported("IGraph DLL loader"))
        );
        assert_eq!(s.archives.iter().flatten().count(), 14);
        assert_eq!(fatal, 0xa5);
        assert!(!f.effects.iter().any(|e| matches!(
            e,
            SystemEffect::ReleaseArchive(_) | SystemEffect::GraphNotFoundMessage
        )));
    }
    #[test]
    fn partial_shutdown_releases_slots_after_subsystems_and_clears_driver_globals() {
        let mut f = ArchiveFixture::default();
        let mut s = SystemState {
            domain: None,
            archives: [Some(SystemHandle(7)); 14],
            graph: Some(SystemHandle(1)),
            sound: Some(SystemHandle(2)),
            driver: Some(SystemHandle(3)),
        };
        s.close_system(&mut f).unwrap();
        assert_eq!(s.archives, [None; 14]);
        assert_eq!((s.graph, s.sound, s.driver), (None, None, None));
        let tail = &f.effects[12..];
        assert_eq!(
            &tail[..5],
            &[
                SystemEffect::DriverClose(SystemHandle(3)),
                SystemEffect::ForceFeedbackClose,
                SystemEffect::CloseJoy,
                SystemEffect::SoundClose(SystemHandle(2)),
                SystemEffect::GraphClose(SystemHandle(1))
            ]
        );
        assert_eq!(f.effects.last(), Some(&SystemEffect::FreeSoundTables));
    }
    #[test]
    fn graph_descriptor_preserves_native_packed_narrowing_and_unknown_error_literal() {
        let mut bytes = OriginalConfig::default().into_bytes();
        bytes[1] = 0x80;
        bytes[10..14].copy_from_slice(&0x1234u32.to_le_bytes());
        bytes[14..18].copy_from_slice(&0x5678u32.to_le_bytes());
        let d = GraphInitDescriptor::from_config(
            &OriginalConfig::from_bytes(&bytes).unwrap(),
            SystemHandle(9),
        );
        assert_eq!(d.adapter, 0xffffff80);
        assert_eq!(d.bit_depth, 0x34);
        assert_eq!(d.refresh_rate, 0x78);
        assert_eq!(d.rect, [50, 50, 800, 600]);
        assert_eq!(d.flags, 0x100b0);
        assert_eq!(
            init_error_text(0x8000000au32 as i32, 4200),
            "error: #4210\r\nFile corupted..."
        );
        assert_eq!(
            init_error_text(123, 4200),
            "error: 0x08X\r\nUnknown graph init error..."
        );
    }
}

#[cfg(test)]
mod joystick_tests {
    use super::*;
    struct Fixture {
        domain: SystemDomainId,
        base_calls: usize,
        count: i32,
        events: Vec<(u64, u32)>,
    }
    impl JoystickBackend for Fixture {
        fn domain_id(&self) -> SystemDomainId {
            self.domain
        }
        fn create_base(
            &mut self,
            _: SystemHandle,
            _: SystemHandle,
        ) -> Result<(bool, Option<SystemHandle>), SystemError> {
            self.base_calls += 1;
            Ok((true, Some(SystemHandle(9))))
        }
        fn num_devices(&mut self, _: SystemHandle) -> Result<i32, SystemError> {
            Ok(self.count)
        }
        fn create_joy(
            &mut self,
            _: SystemHandle,
            index: i32,
            flags: u32,
        ) -> Result<(bool, Option<SystemHandle>), SystemError> {
            assert_eq!(flags, 1);
            Ok((index != 1, Some(SystemHandle(index as u64 + 20))))
        }
        fn set_axis_mode(
            &mut self,
            joy: SystemHandle,
            axis: u32,
            mode: u32,
        ) -> Result<(), SystemError> {
            assert_eq!((axis, mode), (1, 2));
            self.events.push((joy.0, 100));
            Ok(())
        }
        fn send_force_feedback(
            &mut self,
            joy: SystemHandle,
            command: u32,
        ) -> Result<(), SystemError> {
            self.events.push((joy.0, command));
            Ok(())
        }
        fn release_joy(&mut self, joy: SystemHandle) -> Result<(), SystemError> {
            self.events.push((joy.0, 200));
            Ok(())
        }
        fn release_base(&mut self, base: SystemHandle) -> Result<(), SystemError> {
            self.events.push((base.0, 201));
            Ok(())
        }
    }
    #[test]
    fn joystick_domain_rejects_foreign_backend_before_create_or_release() {
        let mut a = Fixture {
            domain: SystemDomainId::default(),
            base_calls: 0,
            count: 1,
            events: vec![],
        };
        let mut b = Fixture {
            domain: SystemDomainId::default(),
            base_calls: 0,
            count: 1,
            events: vec![],
        };
        let mut state = JoystickState::default();
        state
            .init_joy(&mut a, SystemHandle(1), SystemHandle(2))
            .unwrap();
        assert_eq!(
            state.close_joy(&mut b),
            Err(SystemError::ForeignDependencyDomain)
        );
        assert_eq!(
            state.init_joy(&mut b, SystemHandle(1), SystemHandle(2)),
            Err(SystemError::ForeignDependencyDomain)
        );
        assert_eq!(b.base_calls, 0);
        assert!(b.events.is_empty());
        assert_eq!(
            (state.count(), state.capacity(), state.base),
            (1, 1, Some(SystemHandle(9)))
        );
        state.close_joy(&mut a).unwrap();
        assert_eq!((state.count(), state.capacity()), (0, 1));
        assert_eq!(
            state.close_joy(&mut b),
            Err(SystemError::ForeignDependencyDomain)
        );
        assert!(b.events.is_empty());
    }
    #[test]
    fn native_failed_create_inverts_axis_before_append_ff_and_close_retains_capacity() {
        let mut f = Fixture {
            domain: SystemDomainId::default(),
            base_calls: 0,
            count: 3,
            events: vec![],
        };
        let mut s = JoystickState::default();
        s.init_joy(&mut f, SystemHandle(1), SystemHandle(2))
            .unwrap();
        assert_eq!((s.count(), s.capacity()), (3, 4));
        assert_eq!(
            s.live_devices().collect::<Vec<_>>(),
            vec![SystemHandle(20), SystemHandle(21), SystemHandle(22)]
        );
        assert_eq!(
            f.events,
            vec![
                (20, 3),
                (20, 6),
                (20, 4),
                (20, 5),
                (21, 100),
                (21, 3),
                (21, 6),
                (21, 4),
                (21, 5),
                (22, 3),
                (22, 6),
                (22, 4),
                (22, 5)
            ]
        );
        s.close_joy(&mut f).unwrap();
        assert_eq!((s.count(), s.capacity(), s.base), (0, 4, None));
        assert_eq!(
            &f.events[13..],
            &[(20, 200), (21, 200), (22, 200), (9, 201)]
        );
        f.count = 1;
        s.init_joy(&mut f, SystemHandle(1), SystemHandle(2))
            .unwrap();
        assert_eq!((s.count(), s.capacity()), (1, 4));
        s.close_joy(&mut f).unwrap();
        assert_eq!((s.count(), s.capacity()), (0, 4));
    }
}

#[cfg(test)]
mod graph_error_tests {
    use super::*;
    struct ReadOnlyBackend {
        saves: usize,
    }
    impl MessageBoxBackend for ReadOnlyBackend {
        fn message_box(
            &mut self,
            _: Option<SystemHandle>,
            _: &[u8],
            _: Option<&[u8]>,
            _: u32,
        ) -> Result<i32, SystemError> {
            panic!("unreached")
        }
    }
    impl GraphErrorBackend for ReadOnlyBackend {
        fn save_settings(&mut self, c: &OriginalConfig) -> Result<(), SystemError> {
            assert_eq!(c.as_bytes()[28], 0);
            self.saves += 1;
            Err(SystemError::Unsupported("settings persistence"))
        }
        fn clear_graph(&mut self, _: SystemHandle) -> Result<(), SystemError> {
            panic!("must not cross unavailable SaveSettings")
        }
        fn present_graph(&mut self, _: SystemHandle) -> Result<(), SystemError> {
            panic!("unreached")
        }
        fn graph_window(&mut self, _: SystemHandle) -> Result<Option<SystemHandle>, SystemError> {
            panic!("unreached")
        }
        fn close_graph(&mut self, _: SystemHandle) -> Result<(), SystemError> {
            panic!("unreached")
        }
        fn exit_process(&mut self, _: i32) -> Result<std::convert::Infallible, SystemError> {
            panic!("unreached")
        }
    }
    #[test]
    fn unsupported_settings_write_retains_texture_flag_store_before_desktop_calls() {
        let mut b = OriginalConfig::default().into_bytes();
        b[28] = 1;
        let mut c = OriginalConfig::from_bytes(&b).unwrap();
        let mut host = ReadOnlyBackend { saves: 0 };
        assert_eq!(graph_error_handler(-1, &mut c, None, &mut host), Ok(()));
        assert_eq!(host.saves, 0);
        assert_eq!(c.as_bytes()[28], 1);
        assert_eq!(
            graph_error_handler(
                0x80000002u32 as i32,
                &mut c,
                Some(SystemHandle(1)),
                &mut host
            ),
            Err(SystemError::Unsupported("settings persistence"))
        );
        assert_eq!(c.as_bytes()[28], 0);
        assert_eq!(host.saves, 1);
    }
}
