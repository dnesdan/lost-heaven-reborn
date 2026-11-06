//! Host composition of the existing CPU world intake and scene2 prefix loader.
//!
//! This is deliberately not C_mission::Open, MissionRuntime, GameInit, or an
//! compiled-script scheduler. Source-only actor/program intake does not establish a
//! playable mission. All scene, frame, texture and cache effects use the same
//! actual owners; a stopped load retains its first error and executed prefix.
use crate::c_str::StringError;
use crate::city_base_cache::CityBaseCache;
use crate::driver_scene::{
    DriverSceneContext, DriverSceneCreatePhase, DriverSceneError, DriverSceneOwner,
    DriverSceneScreen, NativeSceneSoundDriver,
};
use crate::frame_sound::SoundError;
use crate::game_camera_projection::GameCameraProjection;
use crate::host_scene_adapters::InstallationImages;
use crate::i3d_object::{lightmap_atlas::AtlasStatistics, LitStorageCounts};
use crate::itexture::{CpuTextureStorage, TextureError, TextureResources};
use crate::mission_content_loading::{
    GameGlobalProgramWorldRunner, MissionGlobalProgramError, MissionProgramDiagnostic,
    MissionProgramWorldContext, NamedGlobalProgramError, NamedGlobalProgramOutcome,
    ProgramWorldLocales,
};
use crate::mission_resources::{
    CacheMemoryError, CpuMissionCacheMemory, MissionCacheError, MissionCacheKind, MissionCaches,
};
use crate::mission_scene_loading::{MissionSceneLoadError, MissionSceneLoader, SceneLoadMapPrefix};
use crate::mission_session_navigation::{SessionNavigation, SessionNavigationCause};
use crate::mission_session_roads::{SessionRoads, SessionRoadsCause};
use crate::model_loading::ModelError;
use crate::model_loading::{CpuSceneWorldError, CpuSceneWorldProgress, ModelArena};
use crate::native_precision::Pc24PrecisionState;
use crate::police_state::PoliceGameInitState;
use crate::program_patch::{ProgramPatchStartup, ProgramPatchStartupError};
use crate::runtime_audio::{AudioError, RuntimeAudio};
use crate::scene_ownership::SceneKey;
use crate::scene_resources::{SceneCpuCloseReport, SceneResourceError, SceneResources};
use crate::script_scheduler::{DispatchBudget, FrameReferenceId};
use crate::traffic_generator::TrafficGameFlags;
use std::convert::Infallible;

#[path = "mission_sector_groups.rs"]
mod sector_groups;

/// Explicit probe inputs. Projection and model directories are supplied by
/// the caller; this host adapter does not infer original GameInit.
pub struct MissionLoadRequest<'a> {
    /// Original ModelOpen input, such as `missions/example/scene.i3d`.
    /// The existing adapter performs the evidenced extension rewrite to 4DS.
    pub world_input: &'a str,
    pub scene2_path: &'a str,
    /// Exact AddDir inputs in caller order. Empty means genuinely no directory;
    /// it does not silently install a `Models` search path.
    pub model_directories: &'a [&'a [u8]],
    pub projection: GameCameraProjection,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MissionLoadPhase {
    NotStarted,
    StartupPatches,
    CacheDirectories,
    SoundBackend,
    SceneCreation,
    /// World intake followed by mission weather and primary-sector setup.
    World,
    /// Owned `check.bin` navigation intake in `Open` order (absent files skip).
    Navigation,
    /// Owned `road.bin` network intake in `Open` order (absent files skip).
    Roads,
    LoadMap,
    Scene2Input,
    Scene2Prefix,
    /// The admitted scene2 input, including source actors/programs, completed.
    /// This does not imply GameInit, compiled scripts or gameplay completion.
    PrefixComplete,
}

#[derive(Debug)]
pub enum MissionLoadInitError {
    Textures(TextureError),
    CacheStrings(StringError),
}

#[derive(Debug)]
pub enum MissionLoadCause {
    StartupPatches(ProgramPatchStartupError),
    CacheDirectory {
        kind: MissionCacheKind,
        index: usize,
        error: MissionCacheError<CacheMemoryError>,
    },
    SoundBackend(DriverSceneError<Infallible>),
    SceneCreation {
        phase: DriverSceneCreatePhase,
        /// The screen's generic error is rendered once without changing the
        /// factory's scene identity or the reached constructor phase.
        detail: String,
    },
    World(CpuSceneWorldError),
    Navigation(SessionNavigationCause),
    Roads(SessionRoadsCause),
    LoadMap(String),
    Scene2Input {
        path: String,
        detail: String,
    },
    Scene2Prefix(MissionSceneLoadError),
}

#[derive(Debug)]
pub struct MissionLoadFailure {
    pub phase: MissionLoadPhase,
    pub cause: MissionLoadCause,
}

/// `Stopped` refers to the retained detailed failure returned by `failure()`.
/// Admission failures never replace the first load error or replay its effects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MissionLoadCallError {
    AlreadyEntered,
    TerminalDiscardEntered,
    Stopped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MissionDiscardPhase {
    NotStarted,
    PendingVisual,
    WorldStaging,
    Navigation,
    Roads,
    Cache(MissionCacheKind),
    Listener,
    Scene,
    DriverRegistry,
    RemainingOwners,
    StartupPatches,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MissionDiscardProgress {
    pub phase: MissionDiscardPhase,
    /// Model/Sound/Animation cache teardown follows Scene release. This is a
    /// host ownership order, not original Game CloseSystem.
    pub caches_destructed: usize,
    pub scene_release_executed: bool,
    pub listener_clear_executed: bool,
    pub registry_storage_released: bool,
}
impl Default for MissionDiscardProgress {
    fn default() -> Self {
        Self {
            phase: MissionDiscardPhase::NotStarted,
            caches_destructed: 0,
            scene_release_executed: false,
            listener_clear_executed: false,
            registry_storage_released: false,
        }
    }
}

/// Counts from actual owners, including partial or unregistered scene owners.
/// They are CPU ownership evidence, not native-device or original-runtime proof.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MissionOwnedCounts {
    pub live_frames: usize,
    pub mission_actors: u32,
    pub mission_program_sources: usize,
    pub mission_content_allocations: usize,
    pub mission_car_heap_allocations: usize,
    pub mission_car_cached_tracks: usize,
    pub mission_car_table_allocations: usize,
    pub arena_actor_data_records: usize,
    pub driver_scenes: usize,
    pub driver_scene_capacity: u32,
    pub cache_allocations: usize,
    pub cache_bytes: usize,
    pub scene_graphics: usize,
    pub scene_pixels: usize,
    pub scene_strings: usize,
    pub scene_textures: usize,
    pub audio_sounds: usize,
    pub audio_voices: usize,
    pub audio_sources: usize,
    pub audio_listener_pose: bool,
    pub driver_sound_frames: usize,
    pub driver_listeners: usize,

    pub visual_graphics: usize,
    pub visual_textures: usize,
    pub visual_allocations: usize,
    /// Real managed lit-LOD records and active buffer owner leases. A sticky
    /// interrupted record remains nonzero even after a callback freed a prefix.
    pub visual_lit_storage: LitStorageCounts,
    pub visual_lightmaps: AtlasStatistics,
    /// All six levels and ten slots, including inactive and interrupted prefixes.
    pub saved_lighting_lods: usize,
    pub pending_visual_creator: usize,
    pub pending_factory_creator: usize,
    pub sector_allocations: usize,
    pub portal_allocations: usize,
    pub occluder_allocations: usize,
    pub weather_references: i32,
    /// Session-owned navigation domain: component plus node/connection arrays.
    pub navigation_allocations: usize,
    pub navigation_bytes: usize,
    /// Session-owned road network: parsed node/link arrays.
    pub road_nodes: usize,
    pub road_links: usize,
    /// Session-owned base cache: open blocks and model sets.
    pub base_blocks: usize,
    pub base_model_sets: usize,
}

#[derive(Debug)]
pub enum MissionDiscardCause {
    StartupPatches(ProgramPatchStartupError),
    PendingVisual(ModelError),
    VisualBuffers(ModelError),
    WorldStaging(DriverSceneError<Infallible>),
    Cache {
        kind: MissionCacheKind,
        error: MissionCacheError<CacheMemoryError>,
    },
    UnregisteredListener(DriverSceneError<Infallible>),
    Audio(AudioError),
    Diagnostics(MissionOwnerDiagnosticsError),
    Navigation(SessionNavigationCause),
    Roads(SessionRoadsCause),
    SceneOwner(MissionSceneSelectionError),
    RegisteredScene(DriverSceneError<Infallible>),
    UnregisteredScene(SceneResourceError),
    DriverRegistry(DriverSceneError<Infallible>),
    RemainingOwners(Box<MissionOwnedCounts>),
}

#[derive(Debug)]
pub enum MissionOwnerDiagnosticsError {
    Audio(AudioError),
    SoundRegistry(SoundError),
}

/// An absent Scene is `Ok(None)`; a selected invalid owner is never absence.
#[derive(Debug)]
pub enum MissionSceneSelectionError {
    ForeignReference(FrameReferenceId),
    InvalidScene {
        reference: FrameReferenceId,
        error: ModelError,
    },
}

impl MissionOwnedCounts {
    /// Read one ownership domain shared by the GUI Scene and headless probe.
    /// These borrow the actual owners; no second Scene, cache, or PCM state is made.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_owners(
        arena: &ModelArena,
        driver: &DriverSceneOwner,
        resources: &SceneResources,
        textures: &TextureResources,
        texture_storage: &CpuTextureStorage,
        cache_memory: &CpuMissionCacheMemory,
        audio: &RuntimeAudio,
        loader: Option<&MissionSceneLoader>,
        navigation: Option<&SessionNavigation>,
        roads: Option<&SessionRoads>,
        base_cache: Option<&CityBaseCache>,
    ) -> Result<Self, MissionOwnerDiagnosticsError> {
        let visual = arena.loaded_visual_resources();
        let visual_backend = visual.map(|owner| owner.backend());
        let sound_backend = driver.host_sound_backend();
        Ok(MissionOwnedCounts {
            live_frames: arena.live_frame_count(),
            arena_actor_data_records: arena.live_actor_data_record_count(),
            mission_actors: loader.map_or(0, |loader| loader.content.actors.world.actor_count),
            mission_program_sources: loader.map_or(0, |loader| loader.content.live_program_count()),
            mission_content_allocations: loader
                .map_or(0, |loader| loader.content.live_allocation_count()),
            mission_car_heap_allocations: loader
                .map_or(0, |loader| loader.content.live_car_allocation_count()),
            mission_car_cached_tracks: loader
                .map_or(0, |loader| loader.content.live_car_track_count()),
            mission_car_table_allocations: loader
                .map_or(0, |loader| loader.content.live_car_table_allocation_count()),
            driver_scenes: driver.scenes().len(),
            driver_scene_capacity: driver.capacity(),
            cache_allocations: cache_memory.live_allocations(),
            cache_bytes: cache_memory.live_bytes(),
            scene_graphics: texture_storage.live_graphics(),
            scene_pixels: texture_storage.live_pixels(),
            scene_strings: texture_storage.live_strings(),
            scene_textures: textures.cached().len() + textures.uncached().len(),
            audio_sounds: audio
                .live_sound_count()
                .map_err(MissionOwnerDiagnosticsError::Audio)?,
            audio_voices: audio
                .live_voice_count()
                .map_err(MissionOwnerDiagnosticsError::Audio)?,
            audio_sources: audio
                .live_source_count()
                .map_err(MissionOwnerDiagnosticsError::Audio)?,
            audio_listener_pose: audio
                .listener_present()
                .map_err(MissionOwnerDiagnosticsError::Audio)?,
            driver_sound_frames: sound_backend
                .map_or(Ok(0), |backend| backend.sounds().map(|sounds| sounds.len()))
                .map_err(MissionOwnerDiagnosticsError::SoundRegistry)?,
            driver_listeners: sound_backend
                .map_or(Ok(0), |backend| {
                    backend.listeners().map(|listeners| listeners.len())
                })
                .map_err(MissionOwnerDiagnosticsError::SoundRegistry)?,
            visual_graphics: visual_backend.map_or(0, |owner| owner.live_graphics()),
            visual_textures: visual_backend.map_or(0, |owner| owner.live_textures()),
            visual_allocations: visual_backend.map_or(0, |owner| owner.live_allocations()),
            visual_lit_storage: visual.map_or_else(LitStorageCounts::default, |owner| {
                owner.objects().lit_storage_counts()
            }),
            visual_lightmaps: visual.map_or_else(AtlasStatistics::default, |owner| {
                owner.lightmap_atlas_statistics()
            }),
            saved_lighting_lods: visual
                .map_or(0, |owner| owner.objects().saved_lighting_lod_count()),
            pending_visual_creator: usize::from(
                loader
                    .as_ref()
                    .is_some_and(|loader| loader.pending_visual_replacement.is_some()),
            ),
            pending_factory_creator: usize::from(
                loader
                    .as_ref()
                    .is_some_and(|loader| loader.pending_factory_creator.is_some()),
            ),
            sector_allocations: arena.sector_storage().live_allocations(),
            portal_allocations: arena.live_portal_allocations(),
            occluder_allocations: arena.occluder_storage().live_allocations(),
            weather_references: resources.ws_ref_count(),
            navigation_allocations: navigation.map_or(0, |nav| nav.live_allocations()),
            navigation_bytes: navigation.map_or(0, |nav| nav.live_bytes()),
            road_nodes: roads.map_or(0, |roads| roads.node_count()),
            road_links: roads.map_or(0, |roads| roads.link_count()),
            base_blocks: base_cache.map_or(0, |cache| cache.blocks().len()),
            base_model_sets: base_cache.map_or(0, |cache| cache.model_sets().len()),
        })
    }
}

#[derive(Debug)]
pub struct MissionDiscardFailure {
    pub phase: MissionDiscardPhase,
    pub cause: MissionDiscardCause,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MissionDiscardCallError {
    AlreadyEntered,
    ProgramRuntimeRetained,
    Stopped,
}

/// One terminal host loading attempt over one Installation and one ownership
/// domain. Read-only getters expose evidence; there is no mutable escape hatch
/// for replacing its arena, cache memory, textures, or driver underneath a load.
///
/// Dropping this host value frees Rust storage but does not execute or claim
/// original destructors. `discard_terminal` is explicit and may stop at a real
/// unavailable cleanup boundary. A fresh attempt requires a new session.
pub struct MissionLoadSession {
    installation: lhr_assets::Installation,
    audio: RuntimeAudio,
    driver: DriverSceneOwner,
    arena: ModelArena,
    resources: SceneResources,
    textures: TextureResources,
    texture_storage: CpuTextureStorage,
    caches: MissionCaches,
    cache_memory: CpuMissionCacheMemory,
    navigation: SessionNavigation,
    roads: SessionRoads,
    base_cache: CityBaseCache,
    police_init: PoliceGameInitState,
    traffic_flags: TrafficGameFlags,
    /// Native `C_mission::Open` progress mode (`_can_show_progress_` at
    /// 0x646db0): 0 cleared, 1 when `cache.bin` probes present, 2 after
    /// `OpenBin`. Read live by `ShowProgress`; only 1 advances to 2.
    open_cache_progress: i32,
    /// Native `restore_volume` (mission +0x7c): set even when audio is
    /// absent, consumed by the first `Tick` with `skip_frames < 1`.
    open_restore_volume: bool,
    /// Native `opened` (mission +0xc): set only after a successful
    /// `OpenBin`, before the final `cache.bin` open.
    open_mission_opened: bool,
    /// Host-owned analogue of `C_game::initialized`: set by the
    /// `MarkInitialized` step, consulted by the `Init` guard. Native
    /// returns false from `Init` when already initialized; the host
    /// reports the boundary instead of re-running.
    game_initialized: bool,
    /// Native `mission_name` copy (mission +0x68): the parent folder name,
    /// not the `scene2.bin` filename. `None` until `CopyMissionName`.
    open_mission_name: Option<Vec<u8>>,
    /// Host-configured volume used by `RestoreAudio`
    /// (`mafiaLoadSave.settings.sound_volume` in the original). Defaults to
    /// 1.0 pending the settings owner; always read live after `OpenBin`.
    open_configured_volume: f32,
    /// Host markers for the audio mute/restore boundaries. Muting needs a
    /// live audio owner; the exact DirectSound volume curve stays outside
    /// this owner by construction.
    open_muted: bool,
    open_restored: Option<f32>,
    precision: Pc24PrecisionState,
    phase: MissionLoadPhase,
    entered: bool,
    scene: Option<FrameReferenceId>,
    world: Option<CpuSceneWorldProgress>,
    partial_map: Option<SceneLoadMapPrefix>,
    loader: Option<MissionSceneLoader>,
    scene2_bytes: Option<Vec<u8>>,
    failure: Option<MissionLoadFailure>,
    discard: MissionDiscardProgress,
    discard_failure: Option<MissionDiscardFailure>,
    // Rust drops fields in declaration order: source owners precede this table
    // even if a host caller abandons the session without explicit disposal.
    startup_patches: ProgramPatchStartup,
}

impl MissionLoadSession {
    pub fn new(
        installation: lhr_assets::Installation,
        precision: Pc24PrecisionState,
    ) -> Result<Self, MissionLoadInitError> {
        Ok(Self {
            installation,
            startup_patches: ProgramPatchStartup::default(),
            audio: RuntimeAudio::default(),
            driver: DriverSceneOwner::default(),
            arena: ModelArena::default(),
            resources: SceneResources::default(),
            textures: TextureResources::new().map_err(MissionLoadInitError::Textures)?,
            texture_storage: CpuTextureStorage::default(),
            caches: MissionCaches::empty().map_err(MissionLoadInitError::CacheStrings)?,
            cache_memory: CpuMissionCacheMemory::default(),
            navigation: SessionNavigation::new(),
            roads: SessionRoads::new(),
            // Native `base_cache` lives in zero-initialized `.bss`
            // (mafia-re: `C_cache_base_block base_cache` at 0x63d268), so the
            // constructor-retained `unknown_20` starts as zero at boot.
            base_cache: CityBaseCache::new(0),
            // Native police/traffic statics start cleared; `Init` stores
            // into them after `Open`. No teardown: pure scalar state.
            police_init: PoliceGameInitState::default(),
            traffic_flags: TrafficGameFlags::default(),
            open_cache_progress: 0,
            open_restore_volume: false,
            open_mission_opened: false,
            game_initialized: false,
            open_mission_name: None,
            open_configured_volume: 1.0,
            open_muted: false,
            open_restored: None,
            precision,
            phase: MissionLoadPhase::NotStarted,
            entered: false,
            scene: None,
            world: None,
            partial_map: None,
            loader: None,
            scene2_bytes: None,
            failure: None,
            discard: MissionDiscardProgress::default(),
            discard_failure: None,
        })
    }

    fn global_program_source_admission(&self) -> Result<(), String> {
        if self.phase != MissionLoadPhase::PrefixComplete
            || self.discard.phase != MissionDiscardPhase::NotStarted
            || self.loader.is_none()
        {
            return Err(
                "global programs require completed source prefix before terminal disposal".into(),
            );
        }
        Ok(())
    }
    fn global_program_admission(&self) -> Result<(), MissionGlobalProgramError> {
        self.global_program_source_admission()
            .map_err(MissionGlobalProgramError::SourceUnavailable)
    }
    /// Execute one already initialized source-owned program against this
    /// session's actual actors, arena and mutable Scene selection. Required
    /// effects/diagnostics remain supplied by the caller; no scheduler runs.
    pub fn process_program_in_world<B, D>(
        &mut self,
        index: usize,
        delta: i32,
        budget: &mut DispatchBudget,
        effects: &mut B,
        diagnostic: &mut D,
        locales: ProgramWorldLocales,
    ) -> Result<(), MissionGlobalProgramError>
    where
        B: crate::original_program::RuntimeBackend,
        D: MissionProgramDiagnostic,
    {
        self.global_program_admission()?;
        Pc24PrecisionState::run_owner(
            self,
            |session| &mut session.precision,
            |session| {
                let scene = session.scene.as_mut().ok_or_else(|| {
                    MissionGlobalProgramError::SourceUnavailable("session Scene unavailable".into())
                })?;
                session
                    .loader
                    .as_mut()
                    .unwrap()
                    .content
                    .process_program_in_world(
                        index,
                        delta,
                        budget,
                        &mut MissionProgramWorldContext::new(
                            &mut session.arena,
                            scene,
                            effects,
                            diagnostic,
                            locales,
                        ),
                    )
            },
        )
    }
    /// Required Game-global execution precedes current reverse-priority mission
    /// fallback. Both use the same retained session world and Scene slot.
    pub fn global_program_run_in_world<B, D, G>(
        &mut self,
        name: &[u8],
        game: &mut G,
        budget: &mut DispatchBudget,
        effects: &mut B,
        diagnostic: &mut D,
        locales: ProgramWorldLocales,
    ) -> Result<NamedGlobalProgramOutcome, NamedGlobalProgramError>
    where
        B: crate::original_program::RuntimeBackend,
        D: MissionProgramDiagnostic,
        G: GameGlobalProgramWorldRunner<B, D>,
    {
        self.global_program_source_admission()
            .map_err(NamedGlobalProgramError::SourceUnavailable)?;
        Pc24PrecisionState::run_owner(
            self,
            |session| &mut session.precision,
            |session| {
                let scene = session.scene.as_mut().ok_or_else(|| {
                    NamedGlobalProgramError::SourceUnavailable("session Scene unavailable".into())
                })?;
                session
                    .loader
                    .as_mut()
                    .unwrap()
                    .content
                    .global_program_run_in_world(
                        name,
                        game,
                        budget,
                        &mut MissionProgramWorldContext::new(
                            &mut session.arena,
                            scene,
                            effects,
                            diagnostic,
                            locales,
                        ),
                    )
            },
        )
    }
    /// Explicit native global-program loop on this session's actual source
    /// owners. This does not run actor/Game Init or start script scheduling.
    pub fn global_program_game_init<B: crate::original_program::RuntimeBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<
        (),
        crate::mission_content_loading::GlobalProgramError<
            crate::mission_content_loading::MissionGlobalProgramError,
        >,
    > {
        self.global_program_admission()
            .map_err(crate::mission_content_loading::GlobalProgramError::EntryCountUnavailable)?;
        Pc24PrecisionState::run_owner(
            self,
            |session| &mut session.precision,
            |session| {
                session
                    .loader
                    .as_mut()
                    .unwrap()
                    .content
                    .global_program_game_init(backend)
            },
        )
    }
    pub fn global_program_game_done<B: crate::original_program::RuntimeBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<
        (),
        crate::mission_content_loading::GlobalProgramError<
            crate::mission_content_loading::MissionGlobalProgramError,
        >,
    > {
        self.global_program_admission()
            .map_err(crate::mission_content_loading::GlobalProgramError::EntryCountUnavailable)?;
        Pc24PrecisionState::run_owner(
            self,
            |session| &mut session.precision,
            |session| {
                session
                    .loader
                    .as_mut()
                    .unwrap()
                    .content
                    .global_program_game_done(backend)
            },
        )
    }
    /// Explicit host cleanup after a stopped global runtime. Native Done is not
    /// replayed; the source-prefix state remains available for terminal disposal.
    pub fn discard_global_program_runtimes_host(
        &mut self,
    ) -> Result<
        Vec<crate::mission_content_loading::GlobalProgramDiscard>,
        crate::mission_content_loading::MissionGlobalProgramError,
    > {
        self.global_program_admission()?;
        self.loader
            .as_mut()
            .unwrap()
            .content
            .discard_global_program_runtimes_host()
    }
    pub fn phase(&self) -> MissionLoadPhase {
        self.phase
    }
    pub fn startup_patches(&self) -> &ProgramPatchStartup {
        &self.startup_patches
    }
    pub fn failure(&self) -> Option<&MissionLoadFailure> {
        self.failure.as_ref()
    }
    /// Resolve only the current live Scene in this session's actual arena.
    /// Callback redirection to a foreign, stale or non-Scene owner is an error.
    pub fn scene(&self) -> Result<Option<SceneKey>, MissionSceneSelectionError> {
        self.scene
            .map(|reference| {
                let key = SceneKey(crate::frame_transform::FrameKey(reference.identity()));
                if self.arena.lookup_reference(key.root()) != reference {
                    return Err(MissionSceneSelectionError::ForeignReference(reference));
                }
                self.arena.scene(key).map_err(|error| {
                    MissionSceneSelectionError::InvalidScene { reference, error }
                })?;
                Ok(key)
            })
            .transpose()
    }
    /// The single retained Scene selection, including callback redirection.
    /// Execution and disposal validate its actual arena/type when reached.
    pub fn scene_reference(&self) -> Option<FrameReferenceId> {
        self.scene
    }
    /// Retained CPU Close evidence can outlive its Scene. Namespace validation
    /// still precedes this historical report lookup; no live owner is resolved.
    /// The native device virtual remains outside this headless admission.
    pub fn scene_close_report(&self) -> Option<SceneCpuCloseReport> {
        self.scene.and_then(|reference| {
            let key = SceneKey(crate::frame_transform::FrameKey(reference.identity()));
            (self.arena.lookup_reference(key.root()) == reference)
                .then(|| self.resources.cpu_close_report(key))
                .flatten()
        })
    }
    pub fn driver(&self) -> &DriverSceneOwner {
        &self.driver
    }
    pub fn audio(&self) -> &RuntimeAudio {
        &self.audio
    }
    pub fn arena(&self) -> &ModelArena {
        &self.arena
    }
    pub fn resources(&self) -> &SceneResources {
        &self.resources
    }
    pub fn caches(&self) -> &MissionCaches {
        &self.caches
    }
    pub fn cache_memory(&self) -> &CpuMissionCacheMemory {
        &self.cache_memory
    }
    /// Session-owned navigation intake in `Open` order.
    pub fn navigation(&self) -> &SessionNavigation {
        &self.navigation
    }
    /// Session-owned road network intake in `Open` order.
    pub fn roads(&self) -> &SessionRoads {
        &self.roads
    }
    /// Session-owned base-block cache (`Open` pending its REA analysis).
    pub fn base_cache(&self) -> &CityBaseCache {
        &self.base_cache
    }
    /// Session-owned police init state (`GameInit` order position).
    pub fn police_init(&self) -> &PoliceGameInitState {
        &self.police_init
    }
    /// Session-owned traffic game flags (`Init` order position).
    pub fn traffic_flags(&self) -> &TrafficGameFlags {
        &self.traffic_flags
    }

    /// Native `C_mission::Open` progress mode (`_can_show_progress_`).
    pub fn open_cache_progress(&self) -> i32 {
        self.open_cache_progress
    }

    /// Native `restore_volume` flag (mission +0x7c).
    pub fn open_restore_volume(&self) -> bool {
        self.open_restore_volume
    }

    /// Native `opened` flag (mission +0xc).
    pub fn open_mission_opened(&self) -> bool {
        self.open_mission_opened
    }

    /// Native `mission_name` copy, if set.
    pub fn open_mission_name(&self) -> Option<&[u8]> {
        self.open_mission_name.as_deref()
    }

    /// Host-configured volume read live by `RestoreAudio`.
    pub fn open_configured_volume(&self) -> f32 {
        self.open_configured_volume
    }

    /// Whether the mute boundary ran over a live audio owner.
    pub fn open_muted(&self) -> bool {
        self.open_muted
    }

    /// Volume restored over the live audio owner, if any.
    pub fn open_restored(&self) -> Option<f32> {
        self.open_restored
    }

    fn open_terminal_guard(&self) -> Result<(), String> {
        if self.discard.phase != MissionDiscardPhase::NotStarted {
            return Err("terminal discard entered".into());
        }
        Ok(())
    }

    /// Native `strcpy` takes any name; the host requires nonempty, NUL-free
    /// input within a 256-byte quota (path traversal stays the backend's
    /// sandbox domain, disclosed).
    fn validate_open_mission_name(mission: &str) -> Result<(), String> {
        if mission.is_empty() || mission.len() > 256 || mission.bytes().any(|byte| byte == 0) {
            return Err("mission name must be nonempty NUL-free input within 256 bytes".into());
        }
        Ok(())
    }

    fn open_cache_path(mission: &str) -> String {
        format!("missions/{mission}/cache.bin")
    }

    fn open_check_path(mission: &str) -> String {
        format!("missions/{mission}/check.bin")
    }

    fn open_road_path(mission: &str) -> String {
        format!("missions/{mission}/road.bin")
    }

    fn open_scene2_path(mission: &str) -> String {
        format!("missions/{mission}/scene2.bin")
    }

    fn open_world_path(mission: &str) -> String {
        format!("missions/{mission}/scene.i3d")
    }

    /// Store the `CopyMissionName` copy over the live progress owner. The
    /// parent folder name is copied, not the `scene2.bin` filename.
    pub fn set_open_mission_name(&mut self, mission: &str) -> Result<(), String> {
        self.open_terminal_guard()?;
        Self::validate_open_mission_name(mission)?;
        self.open_mission_name = Some(mission.as_bytes().to_vec());
        Ok(())
    }

    /// Store the configured volume read live by `RestoreAudio`. It must be
    /// finite; the original settings owner stays outside this session.
    pub fn set_open_configured_volume(&mut self, volume: f32) -> Result<(), String> {
        self.open_terminal_guard()?;
        if !volume.is_finite() {
            return Err("configured volume must be finite".into());
        }
        self.open_configured_volume = volume;
        Ok(())
    }

    /// `ResetGame` prefix over the live open state. This resets the host
    /// opened/restore/mute markers; full `ResetIntern` game state stays
    /// outside this owner by construction.
    pub fn reset_open_game(&mut self) -> Result<(), String> {
        self.open_terminal_guard()?;
        self.open_mission_opened = false;
        self.open_restore_volume = false;
        self.open_muted = false;
        self.open_restored = None;
        Ok(())
    }

    /// `ClosePrevious` over the live owners: explicit teardown of the
    /// previous navigation, roads and base-cache prefix so a restart
    /// repeats from zero. This covers the session-owned prefix only; the
    /// native full `Close` (game Done, cutscene, DelActors, programs,
    /// scene) has no live owners in this unit — a previously opened
    /// mission retears down through `MissionLifecycle::close_active`,
    /// which the caller composes before this step. A refused teardown
    /// stops with its first error and preserves the reached prefix; it is
    /// never retried implicitly.
    pub fn close_previous_for_open(&mut self) -> Result<(), String> {
        self.open_terminal_guard()?;
        self.navigation
            .destroy()
            .map_err(|cause| format!("navigation teardown: {cause:?}"))?;
        self.roads
            .destroy()
            .map_err(|cause| format!("roads teardown: {cause:?}"))?;
        let _report = self.base_cache.close();
        Ok(())
    }

    /// `ResetPostClose` marker: verify the previous close disposed to zero.
    /// Session-owned navigation, roads and base-cache owners must read
    /// empty/closed here (not the native unknown_78/actor-ref counters,
    /// which have no live owners in this unit); anything else is an
    /// unavailable-dependency error, not a silent skip.
    pub fn reset_post_close_for_open(&self) -> Result<(), String> {
        self.open_terminal_guard()?;
        if self.navigation.live_allocations() != 0 || self.navigation.live_bytes() != 0 {
            return Err("navigation retains live allocations after close".into());
        }
        if self.roads.node_count() != 0 || self.roads.link_count() != 0 {
            return Err("roads retain a live network after close".into());
        }
        if self.base_cache.is_open()
            || !self.base_cache.blocks().is_empty()
            || !self.base_cache.model_sets().is_empty()
        {
            return Err("base cache retains live blocks after close".into());
        }
        Ok(())
    }

    /// `BindVehicleScene` marker over the live driver owner. The vehicle
    /// `SetScene` wiring stays outside this unit; this verifies the driver
    /// owner is reachable and preserves its order without inventing vehicle
    /// state.
    pub fn bind_vehicle_scene_for_open(&self) -> Result<(), String> {
        self.open_terminal_guard()?;
        let _ = self.driver.capacity();
        Ok(())
    }

    /// `PrepareProgress` marker. Progress creation stays a host marker in
    /// this unit; the mode itself is owned by `ProbeCache`/`AdvanceCacheProgress`.
    pub fn prepare_progress_for_open(&mut self) -> Result<(), String> {
        self.open_terminal_guard()?;
        Ok(())
    }

    /// `OpenWorld` presence over the live installation owner. Full
    /// `scene.i3d` intake remains in the `World` phase of `load`; this step
    /// verifies the required world input is present without duplicating it.
    /// The original `scene.i3d` name rewrites to `scene.4ds`; either spelling
    /// satisfies presence here.
    pub fn open_world_present_for_open(&self, mission: &str) -> Result<(), String> {
        self.open_terminal_guard()?;
        Self::validate_open_mission_name(mission)?;
        let primary = Self::open_world_path(mission);
        let rewritten = format!("missions/{mission}/scene.4ds");
        let present = self
            .installation
            .read_optional(&primary)
            .map_err(|error| format!("world source: {error}"))?;
        if present.is_some() {
            return Ok(());
        }
        let fallback = self
            .installation
            .read_optional(&rewritten)
            .map_err(|error| format!("world source: {error}"))?;
        if fallback.is_none() {
            return Err(format!("world input absent: {primary}"));
        }
        Ok(())
    }

    /// `ResetWeather` marker. Full weather reset remains in the `World`
    /// phase of `load` over the live scene owner.
    pub fn reset_weather_for_open(&self) -> Result<(), String> {
        self.open_terminal_guard()?;
        Ok(())
    }

    /// `FillSectorBits` marker. Full sector-group fill remains in the
    /// `World` phase of `load` over the live arena owner.
    pub fn fill_sector_bits_for_open(&self) -> Result<(), String> {
        self.open_terminal_guard()?;
        Ok(())
    }

    /// `LoadNavigation` (`check.bin`) over the live navigation owner in
    /// `Open` order. Absence is the original `ROpen`-failed branch and
    /// succeeds with nothing retained; any other failure stops with its
    /// first error.
    pub fn load_navigation_for_open(&mut self, mission: &str) -> Result<(), String> {
        self.open_terminal_guard()?;
        Self::validate_open_mission_name(mission)?;
        let path = Self::open_check_path(mission);
        self.navigation
            .load(&self.installation, &path)
            .map_err(|cause| format!("navigation load: {cause:?}"))?;
        Ok(())
    }

    /// `LoadRoads` (`road.bin`) over the live roads owner in `Open` order.
    /// Same absence rule as navigation.
    pub fn load_roads_for_open(&mut self, mission: &str) -> Result<(), String> {
        self.open_terminal_guard()?;
        Self::validate_open_mission_name(mission)?;
        let path = Self::open_road_path(mission);
        self.roads
            .load(&self.installation, &path)
            .map_err(|cause| format!("roads load: {cause:?}"))?;
        Ok(())
    }

    /// `ProbeCache`: set progress mode to zero, try `cache.bin`, close it
    /// on success, then set mode to one. This probe does not decide whether
    /// the later open is tried. Returns presence. Source errors are host
    /// interruptions with the zeroed progress preserved.
    pub fn probe_cache_bin_for_open(&mut self, mission: &str) -> Result<bool, String> {
        self.open_terminal_guard()?;
        Self::validate_open_mission_name(mission)?;
        self.open_cache_progress = 0;
        let path = Self::open_cache_path(mission);
        let present = self
            .installation
            .read_optional(&path)
            .map_err(|error| format!("cache probe source: {error}"))?;
        if present.is_some() {
            self.open_cache_progress = 1;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Reread live audio-owner presence. The driver sound backend is the
    /// `isound`-presence analogue; `OpenBin` and rendering may change it, so
    /// callers must not cache this observation.
    pub fn audio_present_for_open(&self) -> bool {
        self.driver.host_sound_backend().is_some()
    }

    /// `MuteAudioAndRender` over the live audio owner. Muting records its
    /// host marker; exact DirectSound curves stay outside this owner. Scene
    /// rendering stays with the scene owner and is verified only as
    /// reachable here, not executed as a gameplay frame.
    pub fn mute_audio_and_render_for_open(&mut self) -> Result<(), String> {
        self.open_terminal_guard()?;
        if !self.audio_present_for_open() {
            return Err("audio owner absent".into());
        }
        self.open_muted = true;
        Ok(())
    }

    /// Set the original `restore_volume` flag even when audio is absent.
    /// Terminal-guarded like the other mutators: after discard no mutation.
    pub fn mark_open_restore_volume(&mut self) -> Result<(), String> {
        self.open_terminal_guard()?;
        self.open_restore_volume = true;
        Ok(())
    }

    /// `OpenBin` (`scene2.bin`) presence and root-validity probe over the
    /// live installation owner. `Ok(false)` is the native nonzero result
    /// (`NOFILE`/`CORRUPTED`) and becomes `OpenBinFailed` with completed
    /// effects preserved; `Err` is a distinct host interruption. Full actor
    /// and program intake remains in the `Scene2Prefix` phase of `load`.
    pub fn open_bin_probe_for_open(&self, mission: &str) -> Result<bool, String> {
        self.open_terminal_guard()?;
        Self::validate_open_mission_name(mission)?;
        let path = Self::open_scene2_path(mission);
        let bytes = self
            .installation
            .read_optional(&path)
            .map_err(|error| format!("scene2 source: {error}"))?;
        let Some(bytes) = bytes else {
            return Ok(false);
        };
        if lhr_assets::scene_actor_stream::scene2_root(&bytes).is_err() {
            return Ok(false);
        }
        Ok(true)
    }

    /// `RestoreAudio` over the live audio owner and the configured volume
    /// that exists after `OpenBin`. Reads both live; never uses a cached
    /// pre-`OpenBin` flag or volume.
    pub fn restore_audio_for_open(&mut self) -> Result<(), String> {
        self.open_terminal_guard()?;
        if !self.audio_present_for_open() {
            return Err("audio owner absent".into());
        }
        if !self.open_configured_volume.is_finite() {
            return Err("configured volume unavailable".into());
        }
        self.open_restored = Some(self.open_configured_volume);
        Ok(())
    }

    /// Read live progress mode: change one to two, leaving every other
    /// value. This occurs after audio restoration, including native
    /// `OpenBin` failure.
    pub fn advance_open_cache_progress(&mut self) -> Result<(), String> {
        self.open_terminal_guard()?;
        if self.open_cache_progress == 1 {
            self.open_cache_progress = 2;
        }
        Ok(())
    }

    /// Mark the mission opened. Only reached after a successful `OpenBin`.
    /// Terminal-guarded: after discard the mark stays rejected.
    pub fn mark_open_mission_opened(&mut self) -> Result<(), String> {
        self.open_terminal_guard()?;
        self.open_mission_opened = true;
        Ok(())
    }

    /// `OpenCache`: try a fresh optional `cache.bin` open regardless of the
    /// probe result. On success consume the retained stream through the
    /// staged `base_cache` intake and close it; genuine absence is the
    /// original empty branch. A staged intake failure closes to zero and
    /// continues as the empty branch (native `base_cache.Open` is void and
    /// `Open` continues); only host interruptions stop with `Err`.
    pub fn open_cache_bin_for_open(&mut self, mission: &str) -> Result<bool, String> {
        self.open_terminal_guard()?;
        Self::validate_open_mission_name(mission)?;
        let path = Self::open_cache_path(mission);
        let bytes = self
            .installation
            .read_optional(&path)
            .map_err(|error| format!("cache source: {error}"))?;
        let Some(bytes) = bytes else {
            return Ok(false);
        };
        match self.base_cache.open_from_bytes(&bytes) {
            Ok(crate::city_base_cache::BaseCacheOpenOutcome::Ok) => Ok(true),
            // Staged non-Ok (partial outcome or intake error): close to zero
            // and continue as the empty branch like the void native call.
            _ => {
                self.base_cache.close();
                Ok(false)
            }
        }
    }

    /// `FinishProgress` marker (`LOAD_PROGRESS_END`). Progress teardown
    /// stays a host marker in this unit.
    pub fn finish_progress_for_open(&mut self) -> Result<(), String> {
        self.open_terminal_guard()?;
        Ok(())
    }

    /// Owned `C_shvestky::GameInit` + traffic-flag stores in `Init` order,
    /// after `Open`. Police init is infallible; traffic reports its
    /// waver-clear boundary when leaving an enabled driver.
    pub fn run_game_init_prefix(
        &mut self,
    ) -> Result<(), crate::traffic_generator::TrafficFlagsError> {
        self.police_init.apply_game_init();
        self.traffic_flags.apply_game_init()?;
        Ok(())
    }

    fn game_init_admission(&self) -> Result<(), String> {
        self.open_terminal_guard()?;
        if self.phase != MissionLoadPhase::PrefixComplete && !self.open_mission_opened {
            return Err("game init requires completed open prefix".into());
        }
        Ok(())
    }

    /// `Init` guard over the live initialized flag. An already-initialized
    /// game reports the native `return false` boundary; otherwise the open
    /// prefix must be complete (loaded prefix or ordered `Open`).
    pub fn guard_game_init_for_init(&self) -> Result<(), String> {
        if self.game_initialized {
            return Err("game already initialized".into());
        }
        self.game_init_admission()
    }
    /// `MarkInitialized` owner: records the initialized flag the guard
    /// consults. Called by the `Init` driver at its step, never implicitly.
    pub fn mark_game_initialized_for_init(&mut self) -> Result<(), String> {
        self.game_init_admission()?;
        self.game_initialized = true;
        Ok(())
    }

    /// `PoliceGameInit` (`C_shvestky::GameInit` 0x499cd0) over the live
    /// police owner in `Init` order. Infallible by construction.
    pub fn apply_police_game_init_for_init(&mut self) -> Result<(), String> {
        self.game_init_admission()?;
        self.police_init.apply_game_init();
        Ok(())
    }

    /// `TrafficFlags` over the live traffic owner in `Init` order. Leaving
    /// an enabled taxi driver reports its waver-clear boundary instead of
    /// silently dropping it.
    pub fn apply_traffic_flags_for_init(&mut self) -> Result<(), String> {
        self.game_init_admission()?;
        self.traffic_flags
            .apply_game_init()
            .map_err(|error| format!("{error:?}"))?;
        Ok(())
    }

    /// `BaseBlocks` over the live base-cache owner (`IsOpen` only;
    /// `PrepareBaseBlocks` 0x5a3990 is itself a stub upstream). Always
    /// succeeds; it only reads the live open flag.
    pub fn read_base_blocks_for_init(&self) -> Result<bool, String> {
        self.game_init_admission()?;
        Ok(self.base_cache.is_open())
    }
    /// Persistent arithmetic status seeded explicitly by the host caller.
    pub fn precision_state(&self) -> Pc24PrecisionState {
        self.precision
    }
    pub fn world_progress(&self) -> Option<&CpuSceneWorldProgress> {
        self.world.as_ref()
    }
    pub fn loader(&self) -> Option<&MissionSceneLoader> {
        self.loader.as_ref()
    }
    /// A failed map initialization retains the already-published prefix even
    /// though no MissionSceneLoader value could yet be constructed.
    pub fn load_map(&self) -> Option<&SceneLoadMapPrefix> {
        self.loader
            .as_ref()
            .map(|loader| &loader.load_map)
            .or(self.partial_map.as_ref())
    }
    pub fn scene2_byte_count(&self) -> Option<usize> {
        self.scene2_bytes.as_ref().map(Vec::len)
    }
    pub fn discard_progress(&self) -> &MissionDiscardProgress {
        &self.discard
    }
    pub fn discard_failure(&self) -> Option<&MissionDiscardFailure> {
        self.discard_failure.as_ref()
    }
    pub fn owned_counts(&self) -> Result<MissionOwnedCounts, MissionOwnerDiagnosticsError> {
        MissionOwnedCounts::from_owners(
            &self.arena,
            &self.driver,
            &self.resources,
            &self.textures,
            &self.texture_storage,
            &self.cache_memory,
            &self.audio,
            self.loader.as_ref(),
            Some(&self.navigation),
            Some(&self.roads),
            Some(&self.base_cache),
        )
    }

    pub fn load<S: DriverSceneScreen>(
        &mut self,
        request: MissionLoadRequest<'_>,
        screen: &mut S,
    ) -> Result<(), MissionLoadCallError>
    where
        S::Error: std::fmt::Debug,
    {
        if self.discard.phase != MissionDiscardPhase::NotStarted {
            return Err(MissionLoadCallError::TerminalDiscardEntered);
        }
        if self.entered {
            return Err(MissionLoadCallError::AlreadyEntered);
        }
        self.entered = true;
        let result = Pc24PrecisionState::run_owner(
            self,
            |session| &mut session.precision,
            |session| session.load_inner(request, screen),
        );
        if let Err(cause) = result {
            self.failure = Some(MissionLoadFailure {
                phase: self.phase,
                cause,
            });
            return Err(MissionLoadCallError::Stopped);
        }
        self.phase = MissionLoadPhase::PrefixComplete;
        Ok(())
    }

    fn load_inner<S: DriverSceneScreen>(
        &mut self,
        request: MissionLoadRequest<'_>,
        screen: &mut S,
    ) -> Result<(), MissionLoadCause>
    where
        S::Error: std::fmt::Debug,
    {
        self.phase = MissionLoadPhase::StartupPatches;
        self.startup_patches
            .initialize(&self.installation)
            .map_err(MissionLoadCause::StartupPatches)?;
        self.phase = MissionLoadPhase::CacheDirectories;
        // Game startup installs Sounds as the first Sound cache directory.
        // Its source Open flags 0x10 are applied by the Sound cache open path.
        self.caches
            .add_dir(MissionCacheKind::Sound, b"Sounds", &mut self.cache_memory)
            .map_err(|error| MissionLoadCause::CacheDirectory {
                kind: MissionCacheKind::Sound,
                index: 0,
                error,
            })?;
        for (index, directory) in request.model_directories.iter().enumerate() {
            self.caches
                .add_dir(MissionCacheKind::Model, directory, &mut self.cache_memory)
                .map_err(|error| MissionLoadCause::CacheDirectory {
                    kind: MissionCacheKind::Model,
                    index,
                    error,
                })?;
        }
        self.phase = MissionLoadPhase::SoundBackend;
        self.driver
            .bind_host_sound_backend(&mut self.arena, &self.audio)
            .map_err(MissionLoadCause::SoundBackend)?;
        self.phase = MissionLoadPhase::SceneCreation;
        let scene = match self.driver.create_scene(
            &mut DriverSceneContext {
                arena: &mut self.arena,
                resources: &mut self.resources,
                textures: &mut self.textures,
                storage: &mut self.texture_storage,
            },
            screen,
            &mut InstallationImages(&self.installation),
            NativeSceneSoundDriver::Initialized,
        ) {
            Ok(scene) => {
                self.scene = Some(self.arena.lookup_reference(scene.root()));
                scene
            }
            Err(failure) => {
                self.scene = failure
                    .scene
                    .map(|scene| self.arena.lookup_reference(scene.root()));
                return Err(MissionLoadCause::SceneCreation {
                    phase: failure.phase,
                    detail: format!("{:?}", failure.error),
                });
            }
        };

        self.phase = MissionLoadPhase::World;
        match self.driver.load_scene_world_cpu(
            &mut self.arena,
            &self.resources,
            scene,
            &self.installation,
            request.world_input,
        ) {
            Ok(progress) => self.world = Some(progress),
            Err(failure) => {
                self.world = Some(failure.progress);
                return Err(MissionLoadCause::World(failure.error));
            }
        }

        // MissionOpen resets existing weather, then disables it, before touching
        // sector groups. Retain a numeric/lifetime failure as the first load
        // error; the session guard prevents replay of its reached state changes.
        self.resources
            .prepare_mission_weather(&self.arena, scene)
            .map_err(|error| {
                MissionLoadCause::World(CpuSceneWorldError::DriverDomain(format!(
                    "mission weather setup: {error:?}"
                )))
            })?;
        // This is mission setup, not a change to the reusable SceneOpen adapter.
        // Navigation and the remaining MissionOpen work are still separate.
        sector_groups::fill_primary_sector_groups(&mut self.arena, scene)
            .map_err(|error| MissionLoadCause::World(CpuSceneWorldError::Model(error)))?;

        // `C_mission::Open` loads `check.bin` after sector-group fill and
        // before roads/`OpenBin`. Absent files skip like `ROpen`-failed.
        self.phase = MissionLoadPhase::Navigation;
        let check_path = match request.scene2_path.rsplit_once('/') {
            Some((dir, _)) => format!("{dir}/check.bin"),
            None => "check.bin".into(),
        };
        self.navigation
            .load(&self.installation, &check_path)
            .map_err(MissionLoadCause::Navigation)?;

        // `C_mission::Open` loads `road.bin` after `check.bin`. Same
        // absence rule; the network format is established by REA (see
        // `road_network.rs`).
        self.phase = MissionLoadPhase::Roads;
        let road_path = match request.scene2_path.rsplit_once('/') {
            Some((dir, _)) => format!("{dir}/road.bin"),
            None => "road.bin".into(),
        };
        self.roads
            .load(&self.installation, &road_path)
            .map_err(MissionLoadCause::Roads)?;

        self.phase = MissionLoadPhase::LoadMap;
        match MissionSceneLoader::new(&self.arena, scene) {
            Ok(mut loader) => {
                // Open copies the mission name, not the scene2 filename. The
                // requested path's immediate parent supplies that actual name;
                // an unqualified file has no supplied context and remains None.
                let mission_path = request.scene2_path.rfind(['/', '\\']).map(|end| {
                    request.scene2_path[..end]
                        .rsplit(['/', '\\'])
                        .next()
                        .unwrap()
                        .as_bytes()
                        .to_vec()
                });
                loader
                    .content
                    .set_program_context(mission_path)
                    .map_err(MissionLoadCause::LoadMap)?;
                self.loader = Some(loader);
            }
            Err(failure) => {
                self.partial_map = Some(failure.prefix);
                return Err(MissionLoadCause::LoadMap(failure.detail));
            }
        }
        self.phase = MissionLoadPhase::Scene2Input;
        self.scene2_bytes = Some(
            self.installation
                .read_original_path(request.scene2_path)
                .map_err(|error| MissionLoadCause::Scene2Input {
                    path: request.scene2_path.into(),
                    detail: error.to_string(),
                })?,
        );

        self.phase = MissionLoadPhase::Scene2Prefix;
        self.loader
            .as_mut()
            .unwrap()
            .load_prefix_with_resource_cache_and_patches(
                &mut self.driver,
                &mut self.arena,
                &self.resources,
                screen,
                request.projection,
                self.scene2_bytes.as_deref().unwrap(),
                &mut self.caches,
                &self.installation,
                &mut self.cache_memory,
                self.startup_patches.table(),
            )
            .map_err(MissionLoadCause::Scene2Prefix)
    }

    /// One explicit terminal disposal attempt. It is neither native unwinding
    /// nor rollback and never makes the stopped load reusable. Each operation
    /// uses the genuine owner; a refused destructor stops this sequence and
    /// leaves both the load failure and cleanup failure available separately.
    pub fn discard_terminal(&mut self) -> Result<(), MissionDiscardCallError> {
        if self.discard.phase != MissionDiscardPhase::NotStarted {
            return Err(MissionDiscardCallError::AlreadyEntered);
        }
        if self
            .loader
            .as_ref()
            .is_some_and(|loader| loader.content.has_retained_program_runtime())
        {
            return Err(MissionDiscardCallError::ProgramRuntimeRetained);
        }
        let result = Pc24PrecisionState::run_owner(
            self,
            |session| &mut session.precision,
            Self::discard_inner,
        );
        if let Err(cause) = result {
            self.discard_failure = Some(MissionDiscardFailure {
                phase: self.discard.phase,
                cause,
            });
            return Err(MissionDiscardCallError::Stopped);
        }
        self.discard.phase = MissionDiscardPhase::Complete;
        Ok(())
    }

    fn discard_inner(&mut self) -> Result<(), MissionDiscardCause> {
        self.discard.phase = MissionDiscardPhase::PendingVisual;
        if let Some(loader) = self.loader.as_mut() {
            loader
                .release_pending_visual_replacement(&mut self.arena)
                .map_err(MissionDiscardCause::PendingVisual)?;
        }
        self.discard.phase = MissionDiscardPhase::WorldStaging;
        if let Some(progress) = self.world.as_mut() {
            // This disposes only the actual temporary model. Already transferred
            // members remain owned by Scene until its subsequent release.
            self.driver
                .discard_scene_world_staging_cpu(&mut self.arena, &self.resources, progress)
                .map_err(MissionDiscardCause::WorldStaging)?;
        }
        self.discard.phase = MissionDiscardPhase::Navigation;
        self.navigation
            .destroy()
            .map_err(MissionDiscardCause::Navigation)?;
        self.discard.phase = MissionDiscardPhase::Roads;
        self.roads.destroy().map_err(MissionDiscardCause::Roads)?;
        // Native base-cache `Close` runs inside mission teardown; the report
        // counts what the scene owner must still unlink and release.
        let _base_cache_close = self.base_cache.close();
        self.discard.phase = MissionDiscardPhase::Scene;
        if let Some(scene) = self.scene().map_err(MissionDiscardCause::SceneOwner)? {
            let registered = self.driver.scenes().contains(&scene);
            if registered {
                self.driver
                    .release_scene(
                        &mut DriverSceneContext {
                            arena: &mut self.arena,
                            resources: &mut self.resources,
                            textures: &mut self.textures,
                            storage: &mut self.texture_storage,
                        },
                        scene,
                    )
                    .map_err(MissionDiscardCause::RegisteredScene)?;
            } else {
                // An interrupted factory can leave an actual constructed Scene
                // before registration. Incomplete construction still refuses
                // this call; no synthetic registry entry or forced release.
                self.resources
                    .release_scene(
                        &mut self.arena,
                        scene,
                        &mut self.textures,
                        &mut self.texture_storage,
                    )
                    .map_err(MissionDiscardCause::UnregisteredScene)?;
            }
            self.discard.scene_release_executed = true;
            if !registered {
                // Registration can fail after the listener was created. The
                // unregistered Scene uses the resource owner above, so drain
                // its backend listener explicitly after successful release.
                self.driver
                    .release_unregistered_scene_listener(scene)
                    .map_err(MissionDiscardCause::UnregisteredListener)?;
            }
        }
        for kind in [
            MissionCacheKind::Model,
            MissionCacheKind::Sound,
            MissionCacheKind::Animation,
        ] {
            self.discard.phase = MissionDiscardPhase::Cache(kind);
            self.caches
                .destruct(kind, &mut self.arena, &mut self.cache_memory)
                .map_err(|error| MissionDiscardCause::Cache { kind, error })?;
            self.discard.caches_destructed += 1;
        }
        self.discard.phase = MissionDiscardPhase::Listener;
        self.audio
            .clear_listener()
            .map_err(MissionDiscardCause::Audio)?;
        self.discard.listener_clear_executed = true;
        self.discard.phase = MissionDiscardPhase::DriverRegistry;
        self.driver
            .release_empty_registry_storage()
            .map_err(MissionDiscardCause::DriverRegistry)?;
        self.discard.registry_storage_released = true;
        self.discard.phase = MissionDiscardPhase::RemainingOwners;
        self.arena
            .reclaim_unused_visual_buffers()
            .map_err(MissionDiscardCause::VisualBuffers)?;
        let actors_dead = self.loader.as_ref().is_none_or(|loader| {
            loader
                .content
                .actors
                .world
                .owned_actor_references()
                .next()
                .is_none()
        });
        if actors_dead && self.arena.live_frame_count() == 0 {
            // External userdata cells have their own Arena lifetime. This host
            // retirement happens only after every actual actor/frame borrower
            // is dead; it reconstructs no native record destructor.
            self.arena
                .retire_actor_data_records_host_after_frames()
                .map_err(MissionDiscardCause::VisualBuffers)?;
        }
        let remaining = self
            .owned_counts()
            .map_err(MissionDiscardCause::Diagnostics)?;
        if remaining != MissionOwnedCounts::default() {
            return Err(MissionDiscardCause::RemainingOwners(Box::new(remaining)));
        }
        self.discard.phase = MissionDiscardPhase::StartupPatches;
        self.startup_patches
            .unload_after_sources()
            .map_err(MissionDiscardCause::StartupPatches)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "mission_load_session_tests.rs"]
mod tests;
