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
pub use sector_groups::fill_primary_sector_groups;

#[path = "mission_freeride.rs"]
mod freeride_start;
pub use freeride_start::{
    FreerideAttempt, FreerideError, FreerideFailure, FreerideOperations, FreeridePresentation,
    FreerideRequest, FreerideResult, FreerideRunError, FreerideScope, FreerideSetupError,
    FreerideSetupProgress, FreerideSetupStep, FreerideStep, FREERIDE_MISSION,
};

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
    /// A reached ordered source-Open boundary outside the shared loaders.
    OpenStep(crate::mission_open_order::OpenStep),
    /// The admitted scene2 input, including source actors/programs, completed.
    /// This does not imply GameInit, compiled scripts or gameplay completion.
    PrefixComplete,
}

#[derive(Debug)]
pub enum MissionLoadInitError {
    Textures(TextureError),
    MenuResources(String),
    CacheStrings(StringError),
    /// `new C_effects` (`C_mission::Init`, GAME 0x53f230): the constructor's
    /// `tables\effects.tbl` load stopped at a host admission boundary.
    Effects(String),
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
    OpenStep {
        step: crate::mission_open_order::OpenStep,
        detail: String,
    },
}

#[derive(Debug)]
pub struct MissionLoadFailure {
    pub phase: MissionLoadPhase,
    pub cause: MissionLoadCause,
}

/// Source-Open admission refusals. They never replace the first retained
/// load error (`failure()`) or replay its effects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MissionLoadCallError {
    AlreadyEntered,
    TerminalDiscardEntered,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MissionDiscardPhase {
    NotStarted,
    LoadingMenus,
    PendingVisual,
    FreerideModels,
    WorldStaging,
    GameDoneLists,
    Collision,
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
    /// Session-owned `gifCollision`: installed level and dynamic grid.
    pub collision_levels: usize,
    pub collision_dynamic_grids: usize,
    /// Session `C_effects`: emitters in use, static emitters and free pool
    /// entries (`Mission_Done`'s); the loaded table is not a mission owner.
    pub effects_mission_entries: usize,
    pub box_placements: usize,
    pub box_destinations: usize,
    pub box_placement_temporaries: usize,
    pub diff_frames: usize,
    pub diff_actors: usize,
    pub ai_segment_clusters: usize,
    pub ai_segments: usize,
    pub ai_pending_segments: usize,
}

#[derive(Debug)]
pub enum MissionDiscardCause {
    StartupPatches(ProgramPatchStartupError),
    LoadingMenus(String),
    PendingVisual(ModelError),
    FreerideModel(ModelError),
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
    /// `C_game::Done`'s `BoxPlacementDel(boxes[i]->frame)` loop (GAME
    /// 0x5b2ee0, `game.cpp:1347-1348`) is unported; the boxes stay listed.
    BoxPlacementDel {
        boxes: usize,
    },
    /// Native Done calls `ClearDiffData` (GAME 0x5af860) at 0x5a408a,
    /// after `Mission_Done` at 0x5a4083 (REA ev_8310cf2e7dab5297cc3853ba876598295188c280b4e7285985e9c0d2005b7985).
    /// Nonempty lists require the unported actor/frame deletion continuation.
    ClearDiffData {
        frames: usize,
        actors: usize,
    },
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
        collision: Option<&crate::game_collision::GameCollision>,
        effects: Option<&crate::session_effects::SessionEffects>,
        game_lists: Option<&crate::game_housekeeping::GameInitLists>,
        segments: Option<&crate::game_ai_segments::CheckSegClusters>,
    ) -> Result<Self, MissionOwnerDiagnosticsError> {
        let collision_counts = collision.map(|owner| owner.counts()).unwrap_or_default();
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
            collision_levels: collision_counts.levels,
            collision_dynamic_grids: collision_counts.dynamic_grids,
            effects_mission_entries: effects.map_or(0, |owner| owner.live_mission_entries()),
            box_placements: game_lists.map_or(0, |lists| lists.boxes.len()),
            box_destinations: game_lists.map_or(0, |lists| lists.box_dests.len()),
            box_placement_temporaries: game_lists.map_or(0, |lists| lists.box_placement_temp.len()),
            diff_frames: game_lists.map_or(0, |lists| lists.diff_frames.len()),
            diff_actors: game_lists.map_or(0, |lists| lists.diff_actors.len()),
            ai_segment_clusters: segments.map_or(0, |owner| owner.clusters().len()),
            ai_segments: segments.map_or(0, |owner| {
                owner
                    .clusters()
                    .iter()
                    .map(|cluster| cluster.segments().iter().flatten().count())
                    .sum()
            }),
            ai_pending_segments: segments
                .map_or(0, |owner| usize::from(owner.pending_segment().is_some())),
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
    installation: std::sync::Arc<lhr_assets::Installation>,
    audio: RuntimeAudio,
    driver: DriverSceneOwner,
    arena: ModelArena,
    resources: SceneResources,
    menu_draw_resources: crate::menu_draw_resources::MenuDrawResources,
    menu_shared_resources: crate::menu_shared_resources::MenuSharedResources,
    menu_layout: Option<crate::menu::MenuComponentCreateContext>,
    caches: MissionCaches,
    cache_memory: CpuMissionCacheMemory,
    navigation: SessionNavigation,
    roads: SessionRoads,
    base_cache: CityBaseCache,
    police_init: PoliceGameInitState,
    traffic_flags: TrafficGameFlags,
    /// The existing C_mission counter owner also supplies the skip_frames
    /// gate read by the first effect in C_mission::Tick (GAME 0x5407e0).
    mission_counters: crate::mission_actor_lists::MissionCounterState,
    /// `gifCollision` (GAME 0x647f48), installed by the new-game
    /// `LoadCollision` step and cleared before the Scene is released.
    collision: crate::game_collision::GameCollision,
    /// The `mafiaLoadSave` (GAME 0x6d4628) fields: the progress fields the
    /// new-game `SaveGameNewGame` step resets, and the loading screen
    /// `progress` (+0x5d4), a 0x40-byte heap object natively that
    /// SaveGameNewGame leaves alone and only `C_game` teardown deletes
    /// (game.cpp:972-975). A process global natively, held here like
    /// `gifCollision`; it survives disposal and is not a mission owner.
    /// Known divergence: natively the screen carries over to the next
    /// session's Open; here each session starts without one.
    load_save_progress: crate::game_loadsave_lifecycle::LoadSaveProgress,
    /// The `freeride` global's `active` byte (GAME 0x6d4558).
    freeride: crate::game_loadsave_lifecycle::FreerideState,
    freeride_selection_adopted: bool,
    car_database: crate::car_database::CarDatabase,
    freeride_setup: FreerideSetupProgress,
    /// Authoritative represented C_game scalar fields used by source-grounded
    /// inlines. C_game itself and the rest of Init still require live owners.
    game_inline: crate::game_housekeeping::GameInlineState,
    /// The C_game vectors `Init` empties (`stop_sounds`, `paused_i_shows`,
    /// `free_shadow_names`, `unknown_140`), owned beside `game_inline`.
    game_lists: crate::game_housekeeping::GameInitLists,
    game_init_consoles: crate::game_init_consoles::GameInitConsoles,
    saved_init_fast_find: Option<crate::game_init_consoles::SavedFastFind>,
    driver_and_consoles_entered: bool,
    segments_load_entered: bool,
    /// `C_crash_obj::count` (GAME 0x63d6d4), session-scoped like the other
    /// process statics. Starts at the zeroed process seed; crash actor
    /// GameInit/GameDone and Tick producers are not composed here. The
    /// standalone CarRuntime has a separate compatibility context.
    crash_object_count: u32,
    /// The shadow-name buffers `free_shadow_names` lists by id; a separate
    /// owner so clearing the list never frees a buffer.
    shadow_name_buffers: crate::game_housekeeping::ShadowNameBuffers,
    /// `mafiaIndicators` (GAME 0x6bf980), the HUD global. Natively it is
    /// constructed at process start (GAME 0x5f6b70 reads `GetLocalTime`), so
    /// the host installs it with its staged clock read; like
    /// `load_save_progress` it owns no mission allocation and survives
    /// disposal. Absent, `Init` stops at its first HUD effect
    /// (`ScoreSetOn`'s `BonusSetCount`).
    indicators: Option<crate::indicators::IndicatorsState>,
    /// The `AIManager` global's police/groups/follow/fight and segment owners
    /// (GAME 0x646c80). Constructed with the session; segment arrays are owned,
    /// while the other represented vectors hold borrowed identities.
    /// Known divergence: natively its
    /// scalars and vectors carry over across missions until `C_game`
    /// teardown (`KonecSmeny`/`Destroy`, game.cpp:1279-1282, unported);
    /// here each session starts from the constructor zeros.
    ai_manager: crate::ai_manager::AiManager,
    /// The `C_player` statics (`can_car_damage` GAME 0x6478b0, `navi_frame`
    /// GAME 0x6478b4). Process globals like `mafiaIndicators`: they own no
    /// mission allocation and survive disposal. Known divergence: natively
    /// they carry over across missions (between Open and Init `navi_frame`
    /// still names the previous mission's frame); here each session starts
    /// zeroed. Init clears both, so nothing reached observes it.
    player_statics: crate::player_actor::PlayerStatics,
    /// `cbLoadScene`'s globals (`menuProgress` GAME 0x647b20, `last_bit`
    /// GAME 0x647b1c). Process globals natively; session-scoped here like
    /// the loading screen: each session starts from their zero/NULL
    /// process-start state.
    load_scene_callback: crate::load_scene_callback::LoadSceneCallback,
    /// The `C_Vehicle::v_light_sector`/`v_sound_sector` statics (GAME
    /// 0x63d890/0x63d880). Process statics natively; session-scoped here,
    /// each session starting from their empty process-start state. They
    /// borrow this session's sector frames, so disposal empties them before
    /// the scene closes, as native `Done`/`DoneIntern` do.
    vehicle_sectors: crate::vehicle_owner::VehicleSectorStatics,
    /// `C_public_physics::unknown_63d78c` (GAME 0x63d78c, mafia-re
    /// `std::vector<uint>`, meaning unrecovered: `Init`'s clear is its only
    /// known access) and `C_actor::posscoll_frame` (GAME 0x63d2a4), the
    /// two class statics `Init`'s `PhysicsClear` span empties.
    /// Session-scoped like the other process statics: each session starts
    /// from their empty process-start state, and their writers
    /// (`AddFrameForPossibleColl`/`DelFrameForPossibleColl`) stay unported.
    public_physics_unknown_63d78c: Vec<u32>,
    posscoll_frames: Vec<crate::actor_registry::PossCollEntry>,
    /// `C_game::sound_arena` (+0x2b3c), the `soundArena` `Init` resets.
    sound_arena: crate::game_housekeeping::SoundArena,
    /// `act_mission->effects` (`C_mission` +0x6c), constructed by
    /// `C_mission::Init` (GAME 0x53f230) before any mission. Known
    /// divergence: natively one owner lives across missions; here each
    /// session constructs it, loading `tables\effects.tbl`. The table
    /// survives disposal; `Mission_Done` empties the mission's emitters.
    effects: crate::session_effects::SessionEffects,
    input: Option<crate::session_input::KeyboardMouseInput>,
    force_feedback: crate::force_feedback::ForceFeedbackState,
    /// What `Init`'s `GlobalProgramStart` did with the mission-global
    /// programs (initialized once, `GameInitStart` run once).
    game_init_programs: owned_program_runtime::GameInitPrograms,
    game_init_car_sounds: i32,
    game_init_weather: Option<(bool, i32)>,
    game_init_reset_entered: bool,
    /// Native `C_mission::Open` progress mode (`_can_show_progress_` at
    /// 0x646db0): 0 cleared, 1 when `cache.bin` probes present, 2 after
    /// `OpenBin`. Read live by `ShowProgress`; only 1 advances to 2.
    open_cache_progress: i32,
    /// Native `progress` argument of `C_mission::Open` (the stack byte
    /// GAME 0x5409d0 tests at 0x540a79): whether `PrepareProgress` creates
    /// the `mafiaLoadSave.progress` loading screen. Set per call by
    /// `open_source_in_order`; false for the standalone step driver.
    open_progress_argument: bool,
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
    /// Native OPENBIN_CORRUPTED detail from the ordered Open, if any.
    open_bin_corruption: Option<crate::mission_scene_loading::MissionSceneLoadError>,
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

/// `cbEnumSectors`' live frame reads over the session arena: the sector
/// type compare, the stored child count and `GetChild`.
struct SessionSectorFrames<'a> {
    arena: &'a crate::model_loading::ModelArena,
}

impl crate::game_housekeeping::SectorEnumFrames for SessionSectorFrames<'_> {
    type Frame = crate::frame_transform::FrameKey;
    type Error = crate::model_loading::ModelError;

    /// Type word 5 is `FrameKind::Sector` alone in the frame-type projection.
    fn is_sector(&self, frame: Self::Frame) -> Result<bool, Self::Error> {
        Ok(self.arena.state(frame)?.kind == crate::frame_transform::FrameKind::Sector)
    }
    fn num_children(&self, frame: Self::Frame) -> Result<u32, Self::Error> {
        Ok(self.arena.links(frame)?.child_count)
    }
    fn child(&self, frame: Self::Frame, index: u32) -> Result<Option<Self::Frame>, Self::Error> {
        // cbEnumSectors pushes its counter (`push esi`); GetChild compares
        // it signed (`jge`) against the count, which `child_at` reproduces
        // for the reinterpreted bits. Below a count of 2^31 every index the
        // walk passes is in range either way.
        self.arena.child_at(frame, index as i32)
    }
}

/// `LoadCollision`'s calls into the live mission Scene: the driver's
/// FindFrameFast over the arena's fast tree, the frame type word, the
/// visual's `SetSpace(true)` vertex bake and the LOD0 `LockVertices`, all on
/// the arena's live frame, object, mesh and vertex owners. Visuals whose
/// `SetSpace` override has no owner here stop the load where reached.
struct SessionCollisionScene<'a> {
    installation: &'a lhr_assets::Installation,
    arena: &'a mut ModelArena,
    scene: Option<SceneKey>,
}

impl crate::game_collision::CollisionLevelScene for SessionCollisionScene<'_> {
    type Error = String;

    fn read_source(&mut self, name: &str) -> Result<Option<Vec<u8>>, String> {
        self.installation
            .read_optional(name)
            .map_err(|error| error.to_string())
    }

    fn find_frame_fast(
        &mut self,
        name: &[u8],
    ) -> Result<Option<crate::frame_transform::FrameKey>, String> {
        let scene = self.scene.ok_or("no mission Scene for FindFrameFast")?;
        self.arena
            .find_frame_fast(scene, name)
            .map_err(|error| format!("{error:?}"))
    }

    /// The type word as the existing frame-field adapters project it.
    fn frame_type(&mut self, frame: crate::frame_transform::FrameKey) -> Result<u32, String> {
        use crate::frame_transform::FrameKind;
        let state = self
            .arena
            .state(frame)
            .map_err(|error| format!("{error:?}"))?;
        Ok(match state.kind {
            FrameKind::Mesh
            | FrameKind::Singlemesh
            | FrameKind::Projector
            | FrameKind::Lensflare => 1,
            FrameKind::Light => 2,
            FrameKind::Camera => 3,
            FrameKind::Sound => 4,
            FrameKind::Sector => 5,
            FrameKind::Dummy => 6,
            FrameKind::Target => 7,
            FrameKind::Model => 9,
            FrameKind::Joint => 10,
            FrameKind::Occluder => 12,
            FrameKind::Area => 14,
            FrameKind::Scene => return Err("a Scene frame has no type word here".into()),
        })
    }

    /// `I3D_object::SetSpace(true)` (LS3DF 0x378b0) on the arena's owners.
    fn set_visual_space_world(
        &mut self,
        frame: crate::frame_transform::FrameKey,
    ) -> Result<(), String> {
        self.arena
            .set_object_space_world(frame)
            .map(|_| ())
            .map_err(|error| format!("I3D_visual::SetSpace(true): {error:?}"))
    }

    /// `GetMesh()->GetLOD(0)->LockVertices(1)`: a live view of the level's
    /// vertex storage, which the bound faces keep.
    fn lock_lod0_vertices(
        &mut self,
        frame: crate::frame_transform::FrameKey,
    ) -> Result<Option<crate::game_collision::LodVertexBuffer>, String> {
        self.arena
            .lock_visual_lod0_vertices(frame)
            .map_err(|error| format!("mesh GetLOD(0)->LockVertices(1): {error:?}"))
    }
}

#[path = "session_source_open.rs"]
mod owned_source_open;
pub use owned_source_open::MissionSourceOpenError;

/// The unowned native step after `EffectsStart`'s box clears.
pub const EFFECTS_START_STOP: &str = "ff_manager.GameInit (GAME 0x4446b0, called at 0x5a0d63): process input has not been adopted from profile startup";

#[path = "session_program_runtime.rs"]
mod owned_program_runtime;
pub use owned_program_runtime::{
    DetectorScheduleFrontier, GameInitProgramDiagnostic, GameInitPrograms,
    SessionActorProgramError, SessionGameDiffPrograms, SessionProgramEffects,
    GAME_INIT_DISPATCH_BUDGET, GAME_INIT_PROGRAM_LOCALES,
};

impl MissionLoadSession {
    pub fn new(
        installation: lhr_assets::Installation,
        precision: Pc24PrecisionState,
    ) -> Result<Self, MissionLoadInitError> {
        let effects = crate::session_effects::SessionEffects::construct(&installation)
            .map_err(MissionLoadInitError::Effects)?;
        let installation = std::sync::Arc::new(installation);
        let menu_draw_resources = crate::menu_draw_resources::MenuDrawResources::from_cpu_factory(
            crate::menu_draw_resources::MenuTextDatabase::new(),
            TextureResources::new().map_err(MissionLoadInitError::Textures)?,
            CpuTextureStorage::default(),
            crate::host_scene_adapters::SharedInstallationImages(std::sync::Arc::clone(
                &installation,
            )),
        )
        .map_err(MissionLoadInitError::MenuResources)?;
        Ok(Self {
            effects,
            installation,
            startup_patches: ProgramPatchStartup::default(),
            audio: RuntimeAudio::default(),
            driver: DriverSceneOwner::default(),
            // The I3D_driver constructor sets RS_FAST_FIND (mafia-re
            // ls3df/I3D/I3d_driver.cpp:218) and Game's driver init sets it
            // again (game/Main.cpp:749), so every named frame enters the
            // fast-find tree that LoadCollision's FindFrameFast walks.
            // C_game::Init saves RS_FAST_FIND, clears it and restores it
            // (game.cpp:361-362, 940); an Init owner must toggle this flag
            // the same way so frames created during Init stay out of it.
            arena: {
                let mut arena = ModelArena::default();
                arena.set_fast_find_enabled(true);
                arena
            },
            resources: SceneResources::default(),
            menu_draw_resources,
            menu_shared_resources: crate::menu_shared_resources::MenuSharedResources::new(),
            menu_layout: None,
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
            mission_counters: crate::mission_actor_lists::MissionCounterState::default(),
            collision: crate::game_collision::GameCollision::default(),
            load_save_progress: Default::default(),
            freeride: Default::default(),
            freeride_selection_adopted: false,
            car_database: Default::default(),
            freeride_setup: Default::default(),
            game_inline: crate::game_housekeeping::GameInlineState::default(),
            game_lists: Default::default(),
            game_init_consoles: Default::default(),
            saved_init_fast_find: None,
            driver_and_consoles_entered: false,
            segments_load_entered: false,
            crash_object_count: 0,
            shadow_name_buffers: Default::default(),
            indicators: None,
            input: None,
            force_feedback: crate::force_feedback::ForceFeedbackState::from_process_state(0, 0, 0),
            ai_manager: Default::default(),
            player_statics: Default::default(),
            load_scene_callback: Default::default(),
            vehicle_sectors: Default::default(),
            public_physics_unknown_63d78c: Vec::new(),
            posscoll_frames: Vec::new(),
            sound_arena: Default::default(),
            game_init_programs: Default::default(),
            game_init_car_sounds: 0,
            game_init_weather: None,
            game_init_reset_entered: false,
            open_cache_progress: 0,
            open_progress_argument: false,
            open_restore_volume: false,
            open_mission_opened: false,
            game_initialized: false,
            open_mission_name: None,
            open_configured_volume: 1.0,
            open_muted: false,
            open_restored: None,
            open_bin_corruption: None,
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
                        )
                        .with_vehicle_sectors(&mut session.vehicle_sectors),
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
                        )
                        .with_vehicle_sectors(&mut session.vehicle_sectors),
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

    /// Explicit host VM retirement for the retained actors before terminal
    /// source disposal. Global programs have their separate existing method;
    /// neither call claims original GameDone or a completed mission restart.
    pub fn discard_actor_program_runtimes_host(
        &mut self,
    ) -> Result<Vec<crate::actor_world::ActorProgramDiscard>, String> {
        self.global_program_source_admission()?;
        self.loader
            .as_mut()
            .unwrap()
            .content
            .actors
            .world
            .discard_embedded_program_runtimes_host()
            .map_err(|error| format!("actor program host discard: {error:?}"))
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
        Self::resolve_scene_reference(&self.arena, &self.scene)
    }

    fn resolve_scene_reference(
        arena: &ModelArena,
        selection: &Option<FrameReferenceId>,
    ) -> Result<Option<SceneKey>, MissionSceneSelectionError> {
        selection
            .map(|reference| {
                let key = SceneKey(crate::frame_transform::FrameKey(reference.identity()));
                if arena.lookup_reference(key.root()) != reference {
                    return Err(MissionSceneSelectionError::ForeignReference(reference));
                }
                arena
                    .scene(key)
                    .map_err(|error| MissionSceneSelectionError::InvalidScene {
                        reference,
                        error,
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

    /// Prepare a renderable frame from the SAME retained mission Scene used
    /// by gameplay and script callbacks. The caller owns presentation/GPU
    /// resources and the monotonically increasing visibility frame sequence;
    /// this does not construct a second Scene or start gameplay scheduling.
    /// Incomplete loads, terminal disposal and invalid viewport extents refuse
    /// before any camera or Scene state is changed.
    pub fn snapshot_loaded_scene(
        &mut self,
        target: [u32; 2],
        sequence: &mut crate::scene_render::SceneSnapshotSequence,
    ) -> Result<crate::scene_render::CanonicalSceneSnapshot, String> {
        self.global_program_source_admission()?;
        if target.contains(&0) || target.iter().any(|&v| v > 8192) {
            return Err("mission snapshot viewport outside supported range".into());
        }
        let number = sequence.next_number()?;
        Pc24PrecisionState::run_owner(
            self,
            |session| &mut session.precision,
            |session| {
                let scene = session
                    .scene()
                    .map_err(|error| format!("mission snapshot scene: {error:?}"))?
                    .ok_or("mission snapshot Scene unavailable")?;
                session
                    .resources
                    .set_viewport(
                        &session.arena,
                        scene,
                        [0, 0, target[0] as i32, target[1] as i32],
                    )
                    .map_err(|error| format!("mission snapshot viewport: {error:?}"))?;
                let camera = session
                    .arena
                    .scene_active_camera(scene)
                    .map_err(|error| format!("mission snapshot camera: {error:?}"))?
                    .ok_or("mission snapshot active camera unavailable")?;
                session
                    .arena
                    .camera_set_aspect_ratio(camera, target[0] as f32 / target[1] as f32)
                    .map_err(|error| format!("mission snapshot aspect: {error:?}"))?;
                let viewport = session
                    .resources
                    .construction(scene)
                    .and_then(|construction| construction.viewport_projection)
                    .ok_or("mission snapshot canonical viewport unavailable")?;
                crate::scene_render::snapshot_scene(
                    &mut session.arena,
                    scene,
                    viewport,
                    target,
                    number,
                )
            },
        )
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

    /// Native `C_mission::SetSkipFrames` counter, read live by the Tick head.
    pub fn set_skip_frames_for_tick(&mut self, frames: i32) -> Result<(), String> {
        self.open_terminal_guard()?;
        self.mission_counters.set_skip_frames(frames);
        Ok(())
    }

    pub fn skip_frames_for_tick(&self) -> i32 {
        self.mission_counters.get_skip_frames()
    }

    /// The first C_mission::Tick effect (GAME RVA 0x1407e0, VA 0x5407e0):
    /// when skip_frames < 1, clear restore_volume BEFORE calling the audio
    /// owner. If the gain cannot be applied, this flag stays consumed, exactly
    /// preserving the reached original state; a retry must never replay it.
    /// Further Tick calls (tshift, Scene, driver, Game and fading objects)
    /// have distinct owners and remain outside this bounded head operation.
    /// This is host PCM behavior, not original DirectSound numeric parity.
    pub fn tick_restore_volume_head(&mut self) -> Result<bool, String> {
        self.open_terminal_guard()?;
        if !self.open_mission_opened
            || (self.entered && self.phase != MissionLoadPhase::PrefixComplete)
        {
            return Err("mission tick requires completed Open".into());
        }
        if !crate::mission_actor_lists::tick_consume_restore_volume(
            &mut self.open_restore_volume,
            self.mission_counters.get_skip_frames(),
        ) {
            return Ok(false);
        }
        if self.audio_present_for_open() {
            self.audio
                .set_master_gain(self.open_configured_volume)
                .map_err(|error| format!("Tick restore audio: {error}"))?;
            self.open_restored = Some(self.open_configured_volume);
        }
        Ok(true)
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

    /// The scene2 header read that made native OpenBin return
    /// OPENBIN_CORRUPTED during the ordered Open, if one did.
    pub fn open_bin_corruption(
        &self,
    ) -> Option<&crate::mission_scene_loading::MissionSceneLoadError> {
        self.open_bin_corruption.as_ref()
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

    /// Outer `act_mission->Close()` before the mission `Open` (mafia-re
    /// game/Main.cpp:385, `C_mission::Close` Mission.cpp:510-545). Natively
    /// the intro mission (Main.cpp:373-384) is open here and this is a full
    /// Close; that intro leg is not composed, so this step requires a fresh
    /// session, where the native `!opened` early return applies. An opened
    /// mission's full Close (game Done, DelActors, programs, scene Close) has
    /// no owner in this session and stops instead.
    pub fn close_before_new_mission(&mut self) -> Result<(), String> {
        self.open_terminal_guard()?;
        if self.open_mission_opened {
            return Err("full C_mission::Close of an opened mission has no live owner".into());
        }
        Ok(())
    }

    /// `gifCollision.LoadCollision("missions\<name>\tree.klz", true)`
    /// (GAME 0x5c2b70, mafia-re game/collision/g_c_glob.cpp:123) after a
    /// successful Open, over the session's `GameCollision` owner and the
    /// live mission Scene. Frame lookup, the frame type, a visual link's
    /// `SetSpace(true)` bake and the LOD0 vertex lock are the arena's. A
    /// visual without an owner for its `SetSpace` or lock stops the step
    /// with the owner left as native leaves it at that point: a `SetSpace`
    /// stop is in the link loop (previous level deleted, `level_size`
    /// stored, no level published); a LOD0-lock stop comes after the publish
    /// at g_c_glob.cpp:164, with the level installed at phase `VolumesBound`.
    /// The step runs in the session's PC24 precision scope, so the sticky
    /// precision state its arithmetic raises (the `SetSpace` bake, the dyn
    /// grid) persists for later steps, as the one x87 status word does.
    pub fn load_collision_for_new_mission(&mut self, mission: &str) -> Result<(), String> {
        Pc24PrecisionState::run_owner(
            self,
            |session| &mut session.precision,
            |session| session.load_collision_for_new_mission_inner(mission),
        )
    }

    fn load_collision_for_new_mission_inner(&mut self, mission: &str) -> Result<(), String> {
        self.open_terminal_guard()?;
        Self::validate_open_mission_name(mission)?;
        if !self.open_mission_opened {
            return Err("collision intake requires a completed Open".into());
        }
        let path = format!("missions/{mission}/tree.klz");
        let scene = self
            .scene()
            .map_err(|error| format!("mission Scene owner: {error:?}"))?;
        let mut live = SessionCollisionScene {
            installation: &self.installation,
            arena: &mut self.arena,
            scene,
        };
        let result = self.collision.load_collision(&path, true, &mut live);
        let progress = self.collision.load_progress();
        // A native early return (absent file, foreign magic) leaves no level
        // and Main continues; only a host stop is an error.
        result.map(|_| ()).map_err(|error| {
            format!(
                "{path}: {error:?}; links visited {}/{} resolved={} SetSpace calls={}",
                progress.links_visited,
                progress.links,
                progress.links_resolved,
                progress.set_space_calls
            )
        })
    }

    /// `mafiaLoadSave.SaveGameNewGame()` (GAME 0x605ff0) at Main.cpp:395,
    /// after `ResetMissionNumbers` and before `game->Init()`, over the
    /// session's `G_LoadSaveClass` progress and `freeride` owners. The
    /// native body has no failure path; only a terminal session stops it.
    pub fn save_game_new_game_for_new_mission(&mut self) -> Result<(), String> {
        self.open_terminal_guard()?;
        crate::game_loadsave_lifecycle::save_game_new_game(
            &mut self.load_save_progress,
            &mut self.freeride,
        );
        Ok(())
    }

    /// The session's `G_LoadSaveClass` new-game progress owner.
    pub fn load_save_progress(&self) -> &crate::game_loadsave_lifecycle::LoadSaveProgress {
        &self.load_save_progress
    }

    /// Read-only inspection of the actual session-owned S_freeride state.
    pub fn freeride(&self) -> &crate::game_loadsave_lifecycle::FreerideState {
        &self.freeride
    }

    /// The session's `gifCollision` owner.
    pub fn collision(&self) -> &crate::game_collision::GameCollision {
        &self.collision
    }

    /// Inline `C_mission::ResetMissionNumbers` (Mission.h:290-294) over the
    /// live counter owner, as main runs it after LoadCollision.
    pub fn reset_mission_numbers_for_new_mission(&mut self) -> Result<(), String> {
        self.open_terminal_guard()?;
        self.mission_counters.reset_mission_numbers();
        Ok(())
    }

    /// Live pending and current mission numbers (`next_mission_number`,
    /// `mission_number`).
    pub fn mission_numbers(&self) -> (i32, i32) {
        (
            self.mission_counters.next_mission_number(),
            self.mission_counters.get_mission_number(),
        )
    }

    /// `ResetGame` prefix over the live open state: the inline
    /// `game->ResetIntern()` (game.h:1014-1018; GAME 0x5409f2 `mov byte
    /// [eax+0x363c], 1` then 0x5409f9 `mov [eax+0x363d], bl` with ebx 0)
    /// over the session's `C_game` scalars, then the host opened/restore/
    /// mute markers.
    pub fn reset_open_game(&mut self) -> Result<(), String> {
        self.open_terminal_guard()?;
        self.game_inline.reset_intern();
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

    /// `PrepareProgress` (GAME 0x540a79-0x540ab7, Mission.cpp:640-641):
    /// with the `progress` argument set and no live `mafiaLoadSave.progress`
    /// screen, create the mission-open screen (`operator new(0x40)`, then
    /// `GM_LoadProgress(0, 0.7f, NULL)` at 0x540aa6, stored at 0x540ab7).
    /// An existing screen is kept, as natively. The `progress` cache mode
    /// itself is owned by `ProbeCache`/`AdvanceCacheProgress`.
    pub fn prepare_progress_for_open(&mut self) -> Result<(), String> {
        self.open_terminal_guard()?;
        if self.open_progress_argument && self.load_save_progress.progress.is_none() {
            use crate::game_loadsave_lifecycle::{LoadProgressScreen, MISSION_OPEN_PROGRESS_PART};
            let mut inputs = SessionLoadProgressInputs {
                installation: &self.installation,
                indicators: self.indicators.as_ref(),
                mission_name: self.open_mission_name.as_deref(),
                menu_layout: self.menu_layout,
                resources: &mut self.menu_draw_resources,
                shared: &mut self.menu_shared_resources,
                arena: &mut self.arena,
                audio: &self.audio,
            };
            let screen =
                LoadProgressScreen::construct_for_load(MISSION_OPEN_PROGRESS_PART, &mut inputs)?;
            self.load_save_progress.progress = Some(screen);
        }
        Ok(())
    }

    /// Supply the loading menu's established layout from a host before source
    /// Open begins. This does not infer any numerical policy from the device.
    pub fn configure_loading_menu_layout(
        &mut self,
        layout: crate::menu::MenuComponentCreateContext,
    ) -> Result<(), String> {
        self.open_terminal_guard()?;
        if self.phase != MissionLoadPhase::NotStarted
            || self.load_save_progress.progress.is_some()
            || self.load_scene_callback.menu_progress().is_some()
            || self.menu_layout.is_some()
        {
            return Err("loading-menu layout requires a fresh session".into());
        }
        if layout.precision == crate::menu::MenuComponentPrecision::Unestablished
            || !layout
                .screen_scale
                .into_iter()
                .all(|scale| scale.is_finite() && scale > 0.0)
            || !layout.left_margin_x.is_finite()
        {
            return Err("loading-menu layout requires admitted scaling and precision".into());
        }
        self.menu_layout = Some(layout);
        Ok(())
    }

    /// Select startup language assets from complete images before source Open.
    pub fn select_loading_menu_startup_language(&mut self) -> Result<(), String> {
        self.open_terminal_guard()?;
        if self.phase != MissionLoadPhase::NotStarted {
            return Err("startup menu language requires a fresh session".into());
        }
        let indicators = self
            .indicators
            .as_mut()
            .ok_or("indicators owner unavailable")?;
        let version = self
            .installation
            .read_optional("tables/version.txt")
            .map_err(|error| format!("startup language version: {error}"))?;
        crate::menu_startup_language::select_startup_menu_language(
            &self.installation,
            crate::original_language::language_code_from_version(
                version.as_deref().unwrap_or_default(),
            ),
            &mut self.menu_draw_resources,
            indicators,
        )
    }

    /// Diagnostic texture/font prefix only, after startup language selection.
    /// Does not complete Indicators::Create, LoadSave::Init or numeric admission.
    pub fn create_loading_indicator_assets(&mut self) -> Result<(), String> {
        if self.discard.phase != MissionDiscardPhase::NotStarted {
            return Err(
                "indicator startup assets unavailable after terminal disposal entry".into(),
            );
        }
        if self.phase != MissionLoadPhase::NotStarted {
            return Err("indicator startup assets require a fresh mission load session".into());
        }
        if self.indicators.is_none() {
            return Err("mafiaIndicators owner is unavailable".into());
        }
        self.menu_draw_resources.create_indicator_assets(
            &mut crate::menu_draw_resources::InstallationFontMetrics(&self.installation),
        )
    }

    /// `OpenWorld` presence over the live installation owner. Full
    /// `scene.i3d` intake is the ordered `open_source_in_order`; this step
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

    /// `ResetWeather` marker of the standalone presence Open. The real reset
    /// runs in `open_source_in_order` over the live scene owner.
    pub fn reset_weather_for_open(&self) -> Result<(), String> {
        self.open_terminal_guard()?;
        Ok(())
    }

    /// `FillSectorBits` marker of the standalone presence Open. The real
    /// fill runs in `open_source_in_order` over the live arena owner.
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
        // Original mute precedes Render and the restore flag store. The host
        // mixer owns the audible effect, while scene Render is still pending.
        self.audio
            .set_master_gain(0.0)
            .map_err(|error| format!("Open mute audio: {error}"))?;
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
    /// and program intake is the `OpenBin` step of `open_source_in_order`.
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
        self.audio
            .set_master_gain(self.open_configured_volume)
            .map_err(|error| format!("Open restore audio: {error}"))?;
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

    /// `ShowProgress` over the live progress mode (GAME 0x540950, mafia-re
    /// `game/mission/Mission.cpp:589-608`): reads `open_cache_progress`
    /// (`_can_show_progress_`) live with the chunk position, then reports
    /// the rescaled bar through the caller-supplied sink. The chunk seek,
    /// I3D/device scenes and the load-screen callback stay deferred.
    pub fn show_progress_for_open<S: MissionProgressSink>(
        &self,
        sink: &mut S,
        chunk_pos: i32,
        chunk_file_size: i32,
    ) {
        forward_mission_progress(sink, chunk_pos, chunk_file_size, self.open_cache_progress);
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

    /// `FinishProgress`: `cbLoadScene(CBM_MISSION_PROGRESS, 1.0f,
    /// LOAD_PROGRESS_END, NULL)` (Mission.cpp:690), whose END branch
    /// destroys the callback's own screen and touches no device; a
    /// `mafiaLoadSave.progress` screen stays for `C_game::Init`.
    pub fn finish_progress_for_open(&mut self) -> Result<(), String> {
        self.open_terminal_guard()?;
        self.report_load_scene(
            crate::load_scene_callback::CBM_MISSION_PROGRESS,
            1.0f32.to_bits(),
            crate::load_scene_callback::LOAD_PROGRESS_END,
            &mut NoLoadingScreenDevice,
        )
    }

    /// One `cbLoadScene` report over the session's owners: the callback's
    /// globals, `mafiaLoadSave.progress`, and the screen-creation inputs
    /// (installation files, `mafiaIndicators.GetLanguage`, the open
    /// mission's name), with the caller's device.
    pub fn report_load_scene(
        &mut self,
        msg: u32,
        prm1: u32,
        prm2: u32,
        device: &mut dyn crate::load_scene_callback::LoadingScreenDevice,
    ) -> Result<(), String> {
        self.open_terminal_guard()?;
        let mut inputs = SessionLoadProgressInputs {
            installation: &self.installation,
            indicators: self.indicators.as_ref(),
            mission_name: self.open_mission_name.as_deref(),
            menu_layout: self.menu_layout,
            resources: &mut self.menu_draw_resources,
            shared: &mut self.menu_shared_resources,
            arena: &mut self.arena,
            audio: &self.audio,
        };
        self.load_scene_callback.cb_load_scene(
            &mut self.load_save_progress.progress,
            msg,
            prm1,
            prm2,
            &mut inputs,
            device,
        )
    }

    /// The parsed world file's (material, frame) entry counts, once the
    /// world intake parsed it.
    pub fn world_source_counts(&self) -> Option<(usize, usize)> {
        self.world.as_ref()?.source_counts()
    }

    /// The live `cbLoadScene` globals.
    pub fn load_scene_callback(&self) -> &crate::load_scene_callback::LoadSceneCallback {
        &self.load_scene_callback
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
        if self.freeride_setup.entered() && !self.freeride_setup.completed() {
            return Err("game init cannot follow interrupted FreerideSetup".into());
        }
        if self.phase != MissionLoadPhase::PrefixComplete
            && (self.entered || !self.open_mission_opened)
        {
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

    /// Reached C_game::Init stores in native order (GAME RVA 0x1a0810):
    /// mission_passed=false, car_sounds=0, then live Scene weather ON/COUNT,
    /// weather_buffer=-1. With no caller renderer, the subsequent sound
    /// SetVolume/Render pair stops at the unavailable render effect. Preserve
    /// the first unsupported effect without claiming completion or mutating
    /// later Init state; without audio the render is skipped natively and
    /// the span continues as in the rendered path. Re-entry is forbidden by
    /// the one-shot Init guard.
    pub fn reset_game_mission_state_for_init(&mut self) -> Result<(), String> {
        self.reset_game_mission_state_inner(|_, _| {
            Err("Init Scene::Render after sound mute unavailable".into())
        })
    }

    /// The same one-shot native Init prefix with a caller-owned Scene render
    /// backend. It delegates to the already reconstructed LS3DF Render entry
    /// over the live Scene, never a second/copied scene or a generated image.
    /// A successful render continues the span through `unknown_140.clear()`;
    /// it does not complete GameInit or start the game's frame scheduler.
    /// No production composition calls it: it does not run the driver's
    /// unlock-all (LS3DF 0x10062030 at 0x100630b9/0x100630c3), so a caller
    /// wiring a real backend must add `publish_runtime_buffers()` first.
    pub fn reset_game_mission_state_with_render<B: crate::scene_projection::SceneRenderBackend>(
        &mut self,
        renderer: &mut B,
    ) -> Result<(), String> {
        self.reset_game_mission_state_inner(|session, scene| {
            let _status = session
                .arena
                .scene_render_with_backend(scene, renderer)
                .map_err(|error| format!("Init Scene::Render: {error:?}"))?;
            // Source C_game::Init ignores the I3D_scene::Render return value.
            Ok(())
        })
    }

    /// Host presentation bridge for the ordered Init driver. Camera/visibility
    /// preparation borrows the same retained Scene at the reached render call,
    /// after weather reads and audio mute. The callback receives presentation
    /// data only; it never becomes a second gameplay owner.
    ///
    /// The snapshot stands in for the face-group preparation of the driver's
    /// render (LS3DF 0x10062030), which then unlocks every buffer before it
    /// draws: `C_VBManager::m_UnlockAllVBuffers` (0x100697e0 on cVBMan
    /// 0x101c5014: each locked buffer's Unlock, data and lock index cleared,
    /// the list reset), then the index manager's (0x1005b960 on cIBMan
    /// 0x101b50f8). The same unlock runs here over the live owners between
    /// the snapshot and the presentation, so the LOD0 locks LoadCollision
    /// keeps are released and the next Lock takes a fresh backend lock. A
    /// failed snapshot stops before the unlock, as no draw is reached.
    pub(crate) fn reset_game_mission_state_with_presentation(
        &mut self,
        target: [u32; 2],
        sequence: &mut crate::scene_render::SceneSnapshotSequence,
        present: &mut impl FnMut(&crate::scene_render::CanonicalSceneSnapshot) -> Result<(), String>,
    ) -> Result<(), String> {
        self.reset_game_mission_state_inner(|session, _scene| {
            let snapshot = session
                .snapshot_loaded_scene(target, sequence)
                .map_err(|error| format!("Init presentation snapshot: {error}"))?;
            session
                .arena
                .publish_runtime_buffers()
                .map_err(|error| format!("Init render unlock-all: {error:?}"))?;
            present(&snapshot).map_err(|error| format!("Init presentation: {error}"))
        })
    }

    fn reset_game_mission_state_inner(
        &mut self,
        render: impl FnOnce(&mut Self, SceneKey) -> Result<(), String>,
    ) -> Result<(), String> {
        self.guard_game_init_for_init()?;
        if self.game_init_reset_entered {
            return Err("mission state reset already entered".into());
        }
        self.game_init_reset_entered = true;
        self.game_inline.set_mission_passed(false);
        self.game_init_car_sounds = 0;
        let scene = self
            .scene()
            .map_err(|error| format!("live scene: {error:?}"))?
            .ok_or("scene unavailable")?;
        let weather = self
            .resources
            .game_init_weather_switches(&self.arena, scene)
            .map_err(|error| format!("weather read: {error:?}"))?;
        self.game_init_weather = Some(weather);
        crate::game_housekeeping::game_init_weather_buffer_reset(&mut self.game_inline);
        if self.audio_present_for_open() {
            self.audio
                .set_master_gain(0.0)
                .map_err(|error| format!("Init sound volume: {error}"))?;
            render(self, scene)?;
            self.open_restore_volume = true;
        }
        self.reset_game_lists_and_flags_for_init()
    }

    /// The rest of the native `ResetMissionState` span after the sound
    /// mute/render block, in Game.exe order (GAME 0x5a08c3-0x5a098c, reached
    /// whether or not audio is present): `stop_sounds`, `paused_i_shows` and
    /// `free_shadow_names` emptied; the direct `score_on = false` store
    /// (0x5a0949); `ScoreSetOn(false)` (0x5b9fa0, tail call to
    /// `mafiaIndicators.BonusSetCount(0x7fffffff)` at 0x5fb010); `score = 0`;
    /// the flag stores 0x5a0960-0x5a097e; `unknown_140` emptied (0x5a098c).
    /// An absent HUD owner stops after the `score_on` store, before the call,
    /// with every earlier clear kept. The clears drop entries only: the
    /// shadow-name buffers stay in their own owner, as native `clear()` drops
    /// the `char*` values without deleting them.
    fn reset_game_lists_and_flags_for_init(&mut self) -> Result<(), String> {
        self.game_lists.stop_sounds.clear();
        self.game_lists.paused_i_shows.clear();
        self.game_lists.free_shadow_names.clear();
        self.game_inline.score_mut().score_on = false;
        let hud = self
            .indicators
            .as_mut()
            .ok_or("Init ScoreSetOn mafiaIndicators owner unavailable")?;
        crate::game_housekeeping::game_init_score_reset(self.game_inline.score_mut(), hud);
        self.game_inline.game_init_flag_resets();
        self.game_lists.unknown_140.clear();
        Ok(())
    }

    /// Install the `mafiaIndicators` HUD global constructed by the host
    /// (GAME 0x5f6b70 with its staged `GetLocalTime` read). One process
    /// global: a second install is refused and keeps the live owner.
    pub fn install_indicators(
        &mut self,
        indicators: crate::indicators::IndicatorsState,
    ) -> Result<(), String> {
        if self.indicators.is_some() {
            return Err("mafiaIndicators already installed".into());
        }
        self.indicators = Some(indicators);
        Ok(())
    }

    /// The live `mafiaIndicators` owner, when installed.
    pub fn indicators(&self) -> Option<&crate::indicators::IndicatorsState> {
        self.indicators.as_ref()
    }

    /// `PoliceBeginShift` over the live `AIManager` owner in `Init` order:
    /// `ZacatekSmeny` (0x53ba50, tail `ai_animitem_manager::Init`
    /// 0x53d9c0), `ai_groups::Init` (0x531130), `ai_follow_manager::Init`
    /// (0x532da0), `ai_fight_manager::Init` (0x534ec0), from the call sites
    /// 0x5a0996-0x5a09b4. None of them can fail natively.
    pub fn police_begin_shift_for_init(&mut self) -> Result<(), String> {
        self.game_init_admission()?;
        self.ai_manager.game_init_police_begin_shift();
        Ok(())
    }

    /// `PlayerNavi` over the live owners in `Init` order (GAME
    /// 0x5a09b9-0x5a09c6, game.cpp:289-291): `_show_progress_(0.0f)` (`push
    /// ebx` with ebx 0, call 0x5a3960), `SetPlayerNaviPoint(NULL)` (`xor
    /// ecx, ecx`, call 0x58f160), then the inline `can_car_damage = false`
    /// store (`mov [0x6478b0], bl`). Without host presentation a live
    /// loading screen stops at its `Present` (GAME 0x5e3d40), after the bar
    /// write, leaving the navigation point and latch unwritten.
    pub fn player_navi_for_init(&mut self) -> Result<(), String> {
        self.player_navi_for_init_presented(&mut |_| {
            Err("GM_LoadProgress::Present (GAME 0x5e3d40) needs host presentation, which the unpresented Init has not".into())
        })
    }

    /// [`MissionLoadSession::player_navi_for_init`] with the host
    /// presentation receiving the loading screen's `Present`.
    pub(crate) fn player_navi_for_init_presented(
        &mut self,
        present: &mut dyn FnMut(
            crate::load_scene_callback::LoadingScreenFrame<'_>,
        ) -> Result<(), String>,
    ) -> Result<(), String> {
        self.game_init_admission()?;
        self.show_progress_for_init(0.0, present)?;
        self.player_statics.set_player_navi_point(None);
        self.player_statics.set_can_car_damage(false);
        Ok(())
    }

    /// `_show_progress_` (GAME 0x5a3960, game.cpp:1033-1040) over the live
    /// `mafiaLoadSave.progress` owner: the credited gate, then
    /// `SetProgressGameInit` (0x5e3cf0) and `Present` (0x5e3d40) through
    /// the host presentation.
    fn show_progress_for_init(
        &mut self,
        fraction: f32,
        present: &mut dyn FnMut(
            crate::load_scene_callback::LoadingScreenFrame<'_>,
        ) -> Result<(), String>,
    ) -> Result<(), String> {
        let live = self.load_save_progress.progress.is_some();
        if !crate::mission_actor_lists::show_progress_gate(live) {
            return Ok(());
        }
        let screen = self
            .load_save_progress
            .progress
            .as_mut()
            .ok_or("Init _show_progress_ screen vanished after its gate")?;
        screen
            .set_progress_game_init(fraction)
            .map_err(|error| format!("Init _show_progress_({fraction:?}): {error}"))?;
        present(crate::load_scene_callback::LoadingScreenFrame::for_init(
            screen,
            &self.menu_draw_resources,
            &self.menu_shared_resources,
        ))
        .map_err(|error| format!("Init _show_progress_({fraction:?}): {error}"))
    }

    /// `VehicleSectors` gate in `Init` order (GAME 0x5a09cc `mov al,
    /// [esi+0x363c]; cmp al, bl; jz 0x5a0a17`, game.cpp:292): reads the live
    /// `intern_load`, which `Open`'s `ResetIntern` set. Clear skips the span
    /// as natively; set reaches the `C_Vehicle::v_light_sector` (0x63d890,
    /// cleared through 0x44b270 at 0x5a09e8) and `v_sound_sector` clears
    /// and `S_hlp::cbEnumSectors` over the primary sector, whose owners are
    /// not ported, so the step stops there with nothing written.
    pub fn vehicle_sectors_for_init(&mut self) -> Result<(), String> {
        self.game_init_admission()?;
        if !self.game_inline.intern_load() {
            return Ok(());
        }
        // GAME 0x5a09d6-0x5a0a12: both erases, then the primary sector of
        // `act_mission->scene` (`[[0x63788c]+0x10]+0x210`) is read and
        // walked; only the walk can stop, after both clears.
        self.vehicle_sectors.clear_light_sectors();
        self.vehicle_sectors.clear_sound_sectors();
        let scene = self
            .scene()
            .map_err(|error| format!("Init cbEnumSectors scene owner: {error:?}"))?
            .ok_or("Init cbEnumSectors: no mission Scene for the primary sector")?;
        // `I3D_scene::primary_sector` (+0x210), the constructor's first
        // retained sector, not the sector ring head.
        let root = self
            .arena
            .scene_primary_sector(scene)
            .map_err(|error| format!("Init cbEnumSectors primary sector: {error:?}"))?
            .frame();
        crate::game_housekeeping::cb_enum_sectors(
            &SessionSectorFrames { arena: &self.arena },
            root,
            &mut self.vehicle_sectors,
        )
        .map_err(|error| format!("Init cbEnumSectors stopped: {error:?}"))
    }

    /// `PhysicsClear` in `Init` order, GAME 0x5a0a17-0x5a0c2c
    /// (`game.cpp:298-328` at pin fe7e746): the `C_public_physics` static
    /// erase (0x564750 copy + empty destroy 0x4d2040 on 0x63d78c), the
    /// `C_game` member stores and clears, then the `C_actor::posscoll_frame`
    /// erase (0x4bc2c0 on 0x63d2a4). Pure stores and POD erases: nothing
    /// here can stop once admitted, and nothing an entry names is released.
    pub fn physics_clear_for_init(&mut self) -> Result<(), String> {
        self.game_init_admission()?;
        self.public_physics_unknown_63d78c.clear();
        self.game_inline.init_physics_clear(&mut self.game_lists);
        self.posscoll_frames.clear();
        Ok(())
    }

    /// `SoundAndLightness` in `Init` order, GAME 0x5a0c4a-0x5a0c93
    /// (`game.cpp:333-337` at pin fe7e746): `sound_arena.Init()` (0x55fcf0
    /// with `ecx = this+0x2b3c`), `LightnessClear()`, `lightness_next = 0`,
    /// the `invisible_actors` erase, then `mafiaIndicators.RadarClear()`
    /// (0x5fa4a0 with `ecx = 0x6bf980`). Without the HUD owner the step
    /// stops at `RadarClear` with the earlier stores kept.
    pub fn sound_and_lightness_for_init(&mut self) -> Result<(), String> {
        self.game_init_admission()?;
        self.sound_arena.init();
        self.game_inline.init_lightness_clear(&mut self.game_lists);
        self.indicators
            .as_mut()
            .ok_or("Init RadarClear (GAME 0x5fa4a0) mafiaIndicators owner unavailable")?
            .radar_clear();
        Ok(())
    }

    /// `IndicatorsGameInit` in `Init` order: `mov ecx, 0x6bf980; call
    /// 0x5f73c0` at 0x5a0cb3-0x5a0cb8 (`game.cpp:341` at pin fe7e746),
    /// `mafiaIndicators.GameInit()` over the live HUD, `C_game` and
    /// `mafiaLoadSave` owners ([`crate::indicators::IndicatorsState::game_init`]).
    /// Without the HUD owner or the profile intake's settings the step stops
    /// before any store.
    pub fn indicators_game_init_for_init(&mut self) -> Result<(), String> {
        self.game_init_admission()?;
        let hud = self
            .indicators
            .as_mut()
            .ok_or("mafiaIndicators.GameInit (GAME 0x5f73c0) mafiaIndicators owner unavailable")?;
        hud.game_init(&self.game_inline, &self.load_save_progress)
            .map_err(|detail| format!("mafiaIndicators.GameInit (GAME 0x5f73c0): {detail}"))
    }

    /// `EffectsStart` in `Init` order (`game.cpp:342-350` at pin fe7e746):
    /// the box-placement clears at GAME 0x5a0cbd-0x5a0d34
    /// ([`crate::game_housekeeping::GameInitLists::init_box_placement_clear`]),
    /// then over `act_mission` (`mov reg, [0x63788c]`):
    /// `effects->LoadStaticParticles()` (`[+0x6c]`, `call 0x44ad30` at
    /// 0x5a0d3e) over `missions\<name>\effects.bin` and the live scene,
    /// `effects->Mission_Start()` (`call 0x449180` at 0x5a0d4b, result
    /// unused) and `web_path->DeRiciConns()` (`[+0x2c]`, `call 0x54cd20` at
    /// 0x5a0d59). The step stops at `ff_manager.GameInit` (`mov ecx,
    /// 0x647b28; call 0x4446b0` at 0x5a0d5e-0x5a0d63), whose owners are
    /// required; every earlier effect is kept on any stop. With explicit
    /// keyboard/mouse input it completes without creating joystick effects.
    pub fn effects_start_for_init(&mut self) -> Result<(), String> {
        self.game_init_admission()?;
        self.game_lists.init_box_placement_clear();
        self.load_static_particles_for_init()?;
        self.effects
            .mission_start(&self.installation)
            .map_err(|detail| format!("C_effects::Mission_Start (GAME 0x449180): {detail}"))?;
        self.navigation
            .de_rici_conns()
            .map_err(|detail| format!("C_web_path::DeRiciConns (GAME 0x54cd20): {detail}"))?;
        let input = self.input.as_mut().ok_or(EFFECTS_START_STOP)?;
        input.initialize_force_feedback(&mut self.force_feedback)?;
        if self.force_feedback.human_effects() {
            return Err("C_human::InitFFEffects: effect creation is unported".into());
        }
        if self.force_feedback.car_effects() {
            return Err("C_car::InitFFEffects: effect creation is unported".into());
        }
        Ok(())
    }

    /// Reached `DiffClear` fragment of `C_game::Init`, after force-feedback
    /// and before `blood_manager.GameInit`. No actor/frame destruction runs.
    pub fn diff_clear_for_init(&mut self) -> Result<(), String> {
        self.game_init_admission()?;
        self.game_inline
            .init_diff_clear(&mut self.game_lists, &mut self.crash_object_count);
        Ok(())
    }

    pub fn crash_object_count(&self) -> u32 {
        self.crash_object_count
    }

    /// GAME Init span 0x5a0dea-0x5a0e4f: blood-manager and borrowed frame
    /// pools, after DiffClear and before saving the driver fast-find state.
    pub fn blood_and_frame_pools_for_init(&mut self) -> Result<(), String> {
        self.game_init_admission()?;
        self.game_lists.init_blood_and_frame_pools();
        Ok(())
    }

    pub fn driver_and_consoles_for_init(&mut self) -> Result<(), String> {
        self.driver_and_consoles_with_time_for_init(&mut || {
            Ok(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .and_then(|duration| i32::try_from(duration.as_secs()).ok())
                .map_or(u32::MAX, |seconds| seconds as u32))
        })
    }

    pub(crate) fn driver_and_consoles_with_time_for_init(
        &mut self,
        time: &mut dyn FnMut() -> Result<u32, String>,
    ) -> Result<(), String> {
        self.game_init_admission()?;
        if self.driver_and_consoles_entered {
            return Err("driver/consoles Init already entered; no replay".into());
        }
        self.driver_and_consoles_entered = true;
        let mut host = SessionConsoleHost {
            arena: &mut self.arena,
            game: &mut self.game_inline,
            time,
        };
        match self.game_init_consoles.driver_and_consoles(&mut host) {
            Ok(saved) => {
                self.saved_init_fast_find = Some(saved);
                Ok(())
            }
            Err(error) => {
                self.saved_init_fast_find = Some(error.saved_fast_find);
                Err(error.to_string())
            }
        }
    }

    pub fn game_init_consoles(&self) -> &crate::game_init_consoles::GameInitConsoles {
        &self.game_init_consoles
    }

    pub fn saved_init_fast_find(&self) -> Option<crate::game_init_consoles::SavedFastFind> {
        self.saved_init_fast_find
    }

    pub fn segments_load_for_init(&mut self) -> Result<bool, String> {
        self.game_init_admission()?;
        if self.segments_load_entered {
            return Err("AIManager.segments.Load already entered; no replay".into());
        }
        self.segments_load_entered = true;
        let name = self
            .open_mission_name
            .as_deref()
            .ok_or("AIManager.segments.Load: act_mission mission_name unset")?;
        let mut io = crate::game_ai_segments_io::InstallationSegmentsIo::new(&self.installation);
        self.ai_manager
            .segments
            .load(name, &mut io)
            .map_err(|error| format!("AIManager.segments.Load: {error:?}"))
    }

    /// `C_effects::LoadStaticParticles` (GAME 0x44ad30) over the session:
    /// the path from the `CopyMissionName` copy (`act_mission+0x68`), the
    /// file through the installation (absence is `ROpen` failing) and
    /// `FindFrame(name, ENUMF_ALL)` over the live scene
    /// ([`crate::model_loading::ModelArena::scene_find_frame`]).
    fn load_static_particles_for_init(&mut self) -> Result<(), String> {
        let name = self.open_mission_name.as_deref().ok_or(
            "C_effects::LoadStaticParticles (GAME 0x44ad30): act_mission mission_name unset",
        )?;
        let path = crate::session_effects::static_particles_path(name)?;
        let file = self.installation.read_optional(&path).map_err(|error| {
            format!("C_effects::LoadStaticParticles (GAME 0x44ad30) source {path}: {error}")
        })?;
        let scene_reference = &self.scene;
        let arena = &self.arena;
        self.effects
            .load_static_particles(file.as_deref(), |frame_name| {
                let scene = Self::resolve_scene_reference(arena, scene_reference)
                    .map_err(|error| {
                        format!("C_effects::LoadStaticParticles (GAME 0x44ad30) scene: {error:?}")
                    })?
                    .ok_or("FindFrame: act_mission has no scene")?;
                arena
                    .scene_find_frame(
                        scene,
                        Some(frame_name),
                        0xffff,
                        crate::frame_lookup::MAX_LOOKUP_VISITS,
                    )
                    .map_err(|error| format!("I3D_scene::FindFrame: {error:?}"))
            })
            .map(|_| ())
    }

    /// The live `C_effects` owner.
    pub fn effects(&self) -> &crate::session_effects::SessionEffects {
        &self.effects
    }

    /// Process-start input ownership, retained across mission disposal like
    /// native controls. Replacing a live startup owner is not yet supported.
    pub fn adopt_keyboard_mouse_input(
        &mut self,
        input: crate::session_input::KeyboardMouseInput,
    ) -> Result<(), String> {
        if self.input.is_some() {
            return Err("process input already adopted from profile startup".into());
        }
        self.input = Some(input);
        Ok(())
    }

    pub fn input(&self) -> Option<&crate::session_input::KeyboardMouseInput> {
        self.input.as_ref()
    }
    pub fn force_feedback(&self) -> &crate::force_feedback::ForceFeedbackState {
        &self.force_feedback
    }

    /// Adopt the `mafiaLoadSave.settings` that `G_LoadSaveClass::Init`'s
    /// profile intake left before the main menu (see
    /// [`crate::game_loadsave_lifecycle::profile_settings_after_load_save_init`]).
    /// Temporary host stand-in: the real composition
    /// ([`crate::game_loadsave_lifecycle::GameLoadSave::init`]) writes
    /// `settings` through `AllDefault`/`ProfileLoad`, but its profile
    /// enumeration, menu and G_Stream owners are unported, so the host runs
    /// this adoption instead. `ProfileChange` (GAME 0x605070) and the
    /// options menu (`g_m_func.cpp:720`) supersede it once ported.
    /// One process-start intake: a second adoption is refused and keeps the
    /// live owner. Like the rest of `load_save_progress` it survives
    /// disposal.
    pub fn adopt_profile_settings(
        &mut self,
        settings: crate::game_loadsave_lifecycle::ProfileSettings,
    ) -> Result<(), String> {
        if self.load_save_progress.settings.is_some() {
            return Err("mafiaLoadSave.settings already set by the profile intake".into());
        }
        self.load_save_progress.settings = Some(settings);
        Ok(())
    }

    /// The live `C_game::sound_arena` owner.
    pub fn sound_arena(&self) -> &crate::game_housekeeping::SoundArena {
        &self.sound_arena
    }

    /// The `C_public_physics::unknown_63d78c` and `C_actor::posscoll_frame`
    /// statics.
    pub fn physics_clear_statics(&self) -> (&[u32], &[crate::actor_registry::PossCollEntry]) {
        (&self.public_physics_unknown_63d78c, &self.posscoll_frames)
    }

    /// The live `C_Vehicle` sector statics owner.
    pub fn vehicle_sectors(&self) -> &crate::vehicle_owner::VehicleSectorStatics {
        &self.vehicle_sectors
    }

    /// Mutable fixture access to the session's `C_Vehicle` sector statics.
    #[cfg(test)]
    pub(crate) fn vehicle_sectors_mut(
        &mut self,
    ) -> &mut crate::vehicle_owner::VehicleSectorStatics {
        &mut self.vehicle_sectors
    }

    /// The live `C_player` statics owner.
    pub fn player_statics(&self) -> crate::player_actor::PlayerStatics {
        self.player_statics
    }

    /// Mutable test access to the `C_player` statics while their script
    /// writers stay unported.
    #[cfg(test)]
    pub(crate) fn player_statics_mut(&mut self) -> &mut crate::player_actor::PlayerStatics {
        &mut self.player_statics
    }

    /// Mutable test access to the `mafiaLoadSave` owner.
    #[cfg(test)]
    pub(crate) fn load_save_progress_mut(
        &mut self,
    ) -> &mut crate::game_loadsave_lifecycle::LoadSaveProgress {
        &mut self.load_save_progress
    }

    /// The live `AIManager` owner, including its owned check-segment clusters.
    pub fn ai_manager(&self) -> &crate::ai_manager::AiManager {
        &self.ai_manager
    }

    /// Mutable test access to the `AIManager` owner while its writers stay
    /// unported.
    #[cfg(test)]
    pub(crate) fn ai_manager_mut(&mut self) -> &mut crate::ai_manager::AiManager {
        &mut self.ai_manager
    }

    /// The live C_game vectors `Init` empties.
    pub fn game_lists(&self) -> &crate::game_housekeeping::GameInitLists {
        &self.game_lists
    }

    /// The live shadow-name buffer owner.
    pub fn shadow_name_buffers(&self) -> &crate::game_housekeeping::ShadowNameBuffers {
        &self.shadow_name_buffers
    }

    /// Exact reached-prefix observations, not a completed GameInit snapshot.
    pub fn game_init_reset_observation(&self) -> (bool, i32, Option<(bool, i32)>, i32) {
        (
            self.game_inline.mission_passed(),
            self.game_init_car_sounds,
            self.game_init_weather,
            self.game_inline.weather_buffer(),
        )
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
        let (textures, storage) = self
            .menu_draw_resources
            .cpu_texture_context()
            .expect("session retains its canonical CPU texture factory");
        MissionOwnedCounts::from_owners(
            &self.arena,
            &self.driver,
            &self.resources,
            textures,
            storage,
            &self.cache_memory,
            &self.audio,
            self.loader.as_ref(),
            Some(&self.navigation),
            Some(&self.roads),
            Some(&self.base_cache),
            Some(&self.collision),
            Some(&self.effects),
            Some(&self.game_lists),
            Some(&self.ai_manager.segments),
        )
    }

    fn bootstrap_source<S: DriverSceneScreen>(
        &mut self,
        model_directories: &[&[u8]],
        screen: &mut S,
    ) -> Result<SceneKey, MissionLoadCause>
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
        for (index, directory) in model_directories.iter().enumerate() {
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
        let (textures, storage) = self
            .menu_draw_resources
            .cpu_texture_context_mut()
            .expect("session retains its canonical CPU texture factory");
        let scene = match self.driver.create_scene(
            &mut DriverSceneContext {
                arena: &mut self.arena,
                resources: &mut self.resources,
                textures,
                storage,
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
        Ok(scene)
    }

    /// The world intake of `scene->Open` with LoadHierarchy's progress
    /// reported in place: each LoadImage `LoadProgress(p)` runs LS3DF's 0.02
    /// gate (`lastProgress` starts at 0 in LoadHierarchy) and a passing
    /// report is `cbLoadScene(CBM_PROGRESS, p, LOAD_PROGRESS_STEP)` over the
    /// session's owners and `device`, before the entry it precedes is read;
    /// LoadHierarchy's END is `cbLoadScene(CBM_PROGRESS, 1.0f,
    /// LOAD_PROGRESS_END)` where the intake reports it (see
    /// `ModelArena::load_scene_world_cpu_reporting`). A refused report
    /// stops the intake there.
    fn load_world_source(
        &mut self,
        scene: SceneKey,
        world_input: &str,
        device: &mut dyn crate::load_scene_callback::LoadingScreenDevice,
    ) -> Result<(), MissionLoadCause> {
        self.phase = MissionLoadPhase::World;
        let Self {
            driver,
            arena,
            resources,
            installation,
            load_scene_callback,
            load_save_progress,
            indicators,
            open_mission_name,
            menu_layout,
            menu_draw_resources,
            menu_shared_resources,
            audio,
            ..
        } = self;
        let mut last_progress = 0.0f32;
        let mut report = |arena: &mut ModelArena,
                          entry: crate::scene_file_load::LoadHierarchyReport|
         -> Result<(), String> {
            let (p, phase) = match entry {
                crate::scene_file_load::LoadHierarchyReport::Step(p) => {
                    if !crate::scene_file_load::load_progress_gate(
                        &mut last_progress,
                        p,
                        crate::scene_file_load::I3DLOAD_PROGRESS,
                    ) {
                        return Ok(());
                    }
                    (p, crate::load_scene_callback::LOAD_PROGRESS_STEP)
                }
                crate::scene_file_load::LoadHierarchyReport::Begin => {
                    (0.0, crate::load_scene_callback::LOAD_PROGRESS_BEGIN)
                }
                crate::scene_file_load::LoadHierarchyReport::End => {
                    (1.0, crate::load_scene_callback::LOAD_PROGRESS_END)
                }
            };
            let mut inputs = SessionLoadProgressInputs {
                installation,
                indicators: indicators.as_ref(),
                mission_name: open_mission_name.as_deref(),
                menu_layout: *menu_layout,
                resources: menu_draw_resources,
                shared: menu_shared_resources,
                arena,
                audio,
            };
            load_scene_callback
                .cb_load_scene(
                    &mut load_save_progress.progress,
                    crate::load_scene_callback::CBM_PROGRESS,
                    p.to_bits(),
                    phase,
                    &mut inputs,
                    device,
                )
                .map_err(|detail| format!("cbLoadScene: {detail}"))
        };
        match driver.load_scene_world_cpu_reporting(
            arena,
            resources,
            scene,
            installation,
            world_input,
            &mut report,
        ) {
            Ok(progress) => self.world = Some(progress),
            Err(failure) => {
                self.world = Some(failure.progress);
                return Err(MissionLoadCause::World(failure.error));
            }
        }
        Ok(())
    }

    fn prepare_source_weather(&mut self, scene: SceneKey) -> Result<(), MissionLoadCause> {
        // Existing mission weather is reset and disabled before sector setup.
        self.resources
            .prepare_mission_weather(&self.arena, scene)
            .map_err(|error| {
                MissionLoadCause::World(CpuSceneWorldError::DriverDomain(format!(
                    "mission weather setup: {error:?}"
                )))
            })?;
        Ok(())
    }

    fn fill_source_sector_groups(&mut self, scene: SceneKey) -> Result<(), MissionLoadCause> {
        sector_groups::fill_primary_sector_groups(&mut self.arena, scene)
            .map_err(|error| MissionLoadCause::World(CpuSceneWorldError::Model(error)))?;
        Ok(())
    }

    fn initialize_source_map(
        &mut self,
        scene: SceneKey,
        scene2_path: &str,
    ) -> Result<(), MissionLoadCause> {
        self.phase = MissionLoadPhase::LoadMap;
        match MissionSceneLoader::new(&self.arena, scene) {
            Ok(mut loader) => {
                // Open copies the mission name, not the scene2 filename. The
                // requested path's immediate parent supplies that actual name;
                // an unqualified file has no supplied context and remains None.
                let mission_path = scene2_path.rfind(['/', '\\']).map(|end| {
                    scene2_path[..end]
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
        Ok(())
    }

    fn load_source_scene2_prefix<S: DriverSceneScreen>(
        &mut self,
        projection: GameCameraProjection,
        screen: &mut S,
    ) -> Result<(), MissionLoadCause>
    where
        S::Error: std::fmt::Debug,
    {
        self.phase = MissionLoadPhase::Scene2Prefix;
        self.loader
            .as_mut()
            .unwrap()
            .load_prefix_with_resource_cache_and_patches(
                &mut self.driver,
                &mut self.arena,
                &self.resources,
                screen,
                projection,
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
        self.discard.phase = MissionDiscardPhase::LoadingMenus;
        self.discard_loading_menus()
            .map_err(MissionDiscardCause::LoadingMenus)?;
        self.discard.phase = MissionDiscardPhase::PendingVisual;
        if let Some(loader) = self.loader.as_mut() {
            loader
                .release_pending_visual_replacement(&mut self.arena)
                .map_err(MissionDiscardCause::PendingVisual)?;
        }
        self.discard.phase = MissionDiscardPhase::FreerideModels;
        self.release_freeride_model_creators()
            .map_err(MissionDiscardCause::FreerideModel)?;
        self.discard.phase = MissionDiscardPhase::WorldStaging;
        if let Some(progress) = self.world.as_mut() {
            // This disposes only the actual temporary model. Already transferred
            // members remain owned by Scene until its subsequent release.
            self.driver
                .discard_scene_world_staging_cpu(&mut self.arena, &self.resources, progress)
                .map_err(MissionDiscardCause::WorldStaging)?;
        }
        // Native `C_game::Done` order over the C_game lists and the effects
        // owner, all before `gifCollision.GameDone()` and the scene release:
        // `LightnessClear` (game.cpp:1126), `effects->Mission_Done()`
        // (game.cpp:1190), then the box placements (game.cpp:1347-1353):
        // `BoxPlacementDel` for each box from the last, `delete` of each
        // destination, and the three clears. `invisible_actors` is not
        // touched by `Done` natively (only `Init` clears it), so it stays.
        self.discard.phase = MissionDiscardPhase::GameDoneLists;
        crate::game_housekeeping::lightness_clear(&mut self.game_lists.lightness);
        self.effects.mission_done();
        if !self.game_lists.diff_frames.is_empty() || !self.game_lists.diff_actors.is_empty() {
            return Err(MissionDiscardCause::ClearDiffData {
                frames: self.game_lists.diff_frames.len(),
                actors: self.game_lists.diff_actors.len(),
            });
        }
        self.ai_manager.segments.clear();
        if !self.game_lists.boxes.is_empty() {
            return Err(MissionDiscardCause::BoxPlacementDel {
                boxes: self.game_lists.boxes.len(),
            });
        }
        // The destinations are opaque borrowed identities until
        // `BoxPlacementDestAdd` is ported; dropping them is the clear.
        self.game_lists.box_dests.clear();
        self.game_lists.boxes.clear();
        self.game_lists.box_placement_temp.clear();
        // Native `C_game::Done` clears the level through
        // `gifCollision.GameDone()` (mafia-re game/mission/game.cpp:1379)
        // before the scene closes; the level names that Scene's frames.
        self.discard.phase = MissionDiscardPhase::Collision;
        self.collision.game_done();
        // Native `C_game::Done` clears `v_sound_sector` (game.cpp:1404) and
        // `DoneIntern` (GAME 0x5a4d20) `v_light_sector` (game.cpp:1446)
        // while the scene is still open; their keys name its sectors.
        self.vehicle_sectors.clear_sound_sectors();
        self.vehicle_sectors.clear_light_sectors();
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
            let (textures, storage) = self
                .menu_draw_resources
                .cpu_texture_context_mut()
                .expect("session retains its canonical CPU texture factory");
            if registered {
                self.driver
                    .release_scene(
                        &mut DriverSceneContext {
                            arena: &mut self.arena,
                            resources: &mut self.resources,
                            textures,
                            storage,
                        },
                        scene,
                    )
                    .map_err(MissionDiscardCause::RegisteredScene)?;
            } else {
                // An interrupted factory can leave an actual constructed Scene
                // before registration. Incomplete construction still refuses
                // this call; no synthetic registry entry or forced release.
                self.resources
                    .release_scene(&mut self.arena, scene, textures, storage)
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

    fn discard_loading_menus(&mut self) -> Result<(), String> {
        let mut inputs = SessionLoadProgressInputs {
            installation: &self.installation,
            indicators: self.indicators.as_ref(),
            mission_name: self.open_mission_name.as_deref(),
            menu_layout: self.menu_layout,
            resources: &mut self.menu_draw_resources,
            shared: &mut self.menu_shared_resources,
            arena: &mut self.arena,
            audio: &self.audio,
        };
        if self
            .load_scene_callback
            .menu_progress()
            .is_some_and(crate::game_loadsave_lifecycle::LoadProgressScreen::has_shared_instance)
        {
            self.load_scene_callback
                .destroy_menu_progress(&mut inputs)?;
        }
        if let Some(screen) = &mut self.load_save_progress.progress {
            if screen.has_shared_instance() {
                screen.destroy(&mut inputs)?;
                self.load_save_progress.progress = None;
            }
        }
        if let Some(step) = self.menu_shared_resources.interrupted_step() {
            return Err(
                crate::menu_shared_resources::MenuSharedResourcesError::Interrupted(step)
                    .to_string(),
            );
        }
        if self.menu_shared_resources.instance_count() != 0 {
            return Err("loading-menu shared instances remain after screen disposal".into());
        }
        self.menu_draw_resources.destroy_indicator_assets()
    }
}

/// `C_mission::ShowProgress` loading-bar contract (GAME 0x540950, mafia-re
/// `game/mission/Mission.cpp:589-608`). Marker-only: the I3D/device scenes,
/// the chunk seek and the `cbLoadScene` owner stay deferred; only the pure
/// state switch over the chunk position is owned here.
///
/// The game's own progress message to the load callback (mafia-re
/// `game/ui/Load_progress.h:11`).
pub const MISSION_PROGRESS_MESSAGE: i32 = 10_000;
/// `CBM_PROGRESS` second parameter for an intermediate report (mafia-re
/// `ls3df/I3D/I3D.h:323-326`).
pub const MISSION_PROGRESS_STEP: u32 = 1;

/// Pure `ShowProgress` value: the chunk position over its file size, then the
/// `_can_show_progress_` switch (GLOBAL GAME 0x646db0, mafia-re
/// `game/mission/Mission.cpp:60-62,598-606`): mode 1 shows the first fifth of
/// the bar (`* 0.2`), mode 2 the remaining four (`* 0.8 + 0.2`), every other
/// mode the raw ratio. An empty file reports a full bar, as the original.
pub fn show_progress_value(chunk_pos: i32, chunk_file_size: i32, can_show_progress: i32) -> f32 {
    let progress = if chunk_file_size != 0 {
        chunk_pos as f32 / chunk_file_size as f32
    } else {
        1.0
    };
    match can_show_progress {
        1 => progress * 0.2,
        2 => progress * 0.8 + 0.2,
        _ => progress,
    }
}

/// `ShowProgress` report word: the value's bits, as the original passes
/// `*(uint *)&progress` to `cbLoadScene`.
pub fn show_progress_bits(chunk_pos: i32, chunk_file_size: i32, can_show_progress: i32) -> u32 {
    show_progress_value(chunk_pos, chunk_file_size, can_show_progress).to_bits()
}

/// Host effect for the `cbLoadScene(CBM_MISSION_PROGRESS, bits,
/// LOAD_PROGRESS_STEP, NULL)` call (mafia-re `game/mission/Mission.cpp:607`,
/// `game/ui/Load_progress.cpp:17-19`). The load-screen owner stays with the
/// caller.
pub trait MissionProgressSink {
    fn report_mission_progress(&mut self, progress_bits: u32, step: u32);
}

/// Route one `ShowProgress` report to the host sink. Pure dispatch: reads the
/// given mode once, with no I3D/device/mission owner involved.
pub fn forward_mission_progress<S: MissionProgressSink>(
    sink: &mut S,
    chunk_pos: i32,
    chunk_file_size: i32,
    can_show_progress: i32,
) {
    sink.report_mission_progress(
        show_progress_bits(chunk_pos, chunk_file_size, can_show_progress),
        MISSION_PROGRESS_STEP,
    );
}

struct SessionConsoleHost<'a> {
    arena: &'a mut ModelArena,
    game: &'a mut crate::game_housekeeping::GameInlineState,
    time: &'a mut dyn FnMut() -> Result<u32, String>,
}

impl crate::game_init_consoles::DriverAndConsolesHost for SessionConsoleHost<'_> {
    type TimeError = String;

    fn fast_find_enabled(&mut self) -> bool {
        self.arena.fast_find_enabled()
    }

    fn set_fast_find_enabled(&mut self, enabled: bool) {
        self.arena.set_fast_find_enabled(enabled);
    }

    fn time_seed(&mut self) -> Result<u32, Self::TimeError> {
        (self.time)()
    }

    fn seed_game_random(&mut self, seed: u32) {
        self.game.seed_random(seed);
    }
}

/// The session's `GM_LoadProgress::Create`/`OnCreate` inputs.
struct SessionLoadProgressInputs<'a> {
    installation: &'a lhr_assets::Installation,
    indicators: Option<&'a crate::indicators::IndicatorsState>,
    mission_name: Option<&'a [u8]>,
    menu_layout: Option<crate::menu::MenuComponentCreateContext>,
    resources: &'a mut crate::menu_draw_resources::MenuDrawResources,
    shared: &'a mut crate::menu_shared_resources::MenuSharedResources,
    arena: &'a mut ModelArena,
    audio: &'a RuntimeAudio,
}

impl crate::game_loadsave_lifecycle::LoadProgressCreateInputs for SessionLoadProgressInputs<'_> {
    fn shared_lifecycle(
        &mut self,
    ) -> Option<&mut dyn crate::game_loadsave_lifecycle::LoadProgressSharedLifecycle> {
        self.menu_layout.is_some().then_some(self)
    }

    fn native_layout(&self) -> Option<crate::menu::MenuComponentCreateContext> {
        self.menu_layout
    }

    fn component_resources(
        &mut self,
    ) -> Option<&mut dyn crate::menu_draw_resources::MenuComponentResources> {
        Some(self.resources)
    }

    fn draw_resources(&self) -> Option<&crate::menu_draw_resources::MenuDrawResources> {
        Some(self.resources)
    }

    fn progress_texture(&self) -> Result<Option<crate::itexture::TextureKey>, String> {
        self.shared
            .texture(crate::menu_shared_resources::MenuSharedTexture::Progress)
            .map_err(|error| error.to_string())
    }

    fn menu_definition(
        &mut self,
        name: &str,
    ) -> Result<Option<lhr_assets::MenuDefinition>, String> {
        let language = self
            .indicators
            .ok_or("GM_Menu::CreateCompsByDef: mafiaIndicators (GetLanguage, GAME 0x5f70d0) owner unavailable")?
            .get_language();
        let [first, second, ..] = language.to_le_bytes();
        let path_language = u32::from(u16::from_be_bytes([first, second]));
        let candidates = lhr_assets::MenuDefinition::menu_file_candidates(name, path_language)
            .ok_or_else(|| format!("menu name {name} is not admittable"))?;
        for path in candidates {
            let bytes = self
                .installation
                .read_optional(&path)
                .map_err(|error| format!("menu definition {path}: {error}"))?;
            if let Some(bytes) = bytes {
                return lhr_assets::parse_menu_definition(&bytes)
                    .map(Some)
                    .map_err(|error| format!("menu definition {path}: {error}"));
            }
        }
        Ok(None)
    }

    fn load_def(&mut self) -> Result<Option<Vec<u8>>, String> {
        let path = crate::game_loadsave_lifecycle::LOAD_DEF_PATH;
        self.installation
            .read_optional(path)
            .map_err(|error| format!("{path}: {error}"))
    }

    fn act_mission_name(&mut self) -> Result<Vec<u8>, String> {
        self.mission_name
            .map(<[u8]>::to_vec)
            .ok_or_else(|| "act_mission->mission_name is NULL".into())
    }
}

impl SessionLoadProgressInputs<'_> {
    fn with_shared<Output>(
        &mut self,
        effect: impl FnOnce(
            &mut crate::menu_shared_resources::MenuSharedResources,
            &mut crate::menu_session_resources::MenuSessionResources<'_>,
        )
            -> Result<Output, crate::menu_shared_resources::MenuSharedResourcesError>,
    ) -> Result<Output, String> {
        let mut host = crate::menu_session_resources::MenuSessionResources::new(
            self.arena,
            self.audio,
            self.resources,
            self.installation,
        );
        effect(self.shared, &mut host).map_err(|error| error.to_string())
    }
}

impl crate::game_loadsave_lifecycle::LoadProgressSharedLifecycle for SessionLoadProgressInputs<'_> {
    fn construct_shared(
        &mut self,
    ) -> Result<crate::menu_shared_resources::MenuSharedInstance, String> {
        self.with_shared(|shared, host| shared.construct(host))
    }

    fn check_shared_instance(
        &self,
        instance: &crate::menu_shared_resources::MenuSharedInstance,
    ) -> Result<(), String> {
        self.shared
            .check_instance(instance)
            .map_err(|error| error.to_string())
    }

    fn check_shared(
        &self,
        instance: &crate::menu_shared_resources::MenuSharedInstance,
    ) -> Result<(), String> {
        self.check_shared_instance(instance)?;
        if let Some(step) = self.shared.interrupted_step() {
            return Err(
                crate::menu_shared_resources::MenuSharedResourcesError::Interrupted(step)
                    .to_string(),
            );
        }
        Ok(())
    }

    fn destroy_shared(
        &mut self,
        instance: &mut crate::menu_shared_resources::MenuSharedInstance,
    ) -> Result<(), String> {
        self.with_shared(|shared, host| shared.destroy(instance, host))
    }

    fn play_open_sound(&mut self, closing: bool) -> Result<(), String> {
        self.with_shared(|shared, host| shared.play_open_sound(host, closing))
    }
}

/// The device of a `cbLoadScene` call that reaches none (`LOAD_PROGRESS_END`).
struct NoLoadingScreenDevice;

impl crate::load_scene_callback::LoadingScreenDevice for NoLoadingScreenDevice {
    fn set_alpha_blend_state(&mut self) -> Result<(), String> {
        Err("loading-screen device unavailable".into())
    }
    fn present_loading_screen(
        &mut self,
        _: crate::load_scene_callback::LoadingScreenFrame<'_>,
    ) -> Result<(), String> {
        Err("loading-screen device unavailable".into())
    }
}

#[cfg(test)]
#[path = "mission_load_session_tests.rs"]
mod tests;
