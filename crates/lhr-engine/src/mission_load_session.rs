//! Host composition of the existing CPU world intake and scene2 prefix loader.
//!
//! This is deliberately not C_mission::Open, MissionRuntime, GameInit, or an
//! compiled-script scheduler. Source-only actor/program intake does not establish a
//! playable mission. All scene, frame, texture and cache effects use the same
//! actual owners; a stopped load retains its first error and executed prefix.
use crate::c_str::StringError;
use crate::driver_scene::{
    DriverSceneContext, DriverSceneCreatePhase, DriverSceneError, DriverSceneOwner,
    DriverSceneScreen, NativeSceneSoundDriver,
};
use crate::frame_sound::SoundError;
use crate::game_camera_projection::GameCameraProjection;
use crate::host_scene_adapters::InstallationImages;
use crate::i3d_object::{lightmap_atlas::AtlasStatistics, LitStorageCounts};
use crate::itexture::{CpuTextureStorage, TextureError, TextureResources};
use crate::mission_resources::{
    CacheMemoryError, CpuMissionCacheMemory, MissionCacheError, MissionCacheKind, MissionCaches,
};
use crate::mission_scene_loading::{MissionSceneLoadError, MissionSceneLoader, SceneLoadMapPrefix};
use crate::model_loading::ModelError;
use crate::model_loading::{CpuSceneWorldError, CpuSceneWorldProgress, ModelArena};
use crate::native_precision::Pc24PrecisionState;
use crate::program_patch::{ProgramPatchStartup, ProgramPatchStartupError};
use crate::runtime_audio::{AudioError, RuntimeAudio};
use crate::scene_ownership::SceneKey;
use crate::scene_resources::{SceneCpuCloseReport, SceneResourceError, SceneResources};
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
    ) -> Result<Self, MissionOwnerDiagnosticsError> {
        let visual = arena.loaded_visual_resources();
        let visual_backend = visual.map(|owner| owner.backend());
        let sound_backend = driver.host_sound_backend();
        Ok(MissionOwnedCounts {
            live_frames: arena.live_frame_count(),
            mission_actors: loader.map_or(0, |loader| loader.content.actors.world.actor_count),
            mission_program_sources: loader.map_or(0, |loader| loader.content.live_program_count()),
            mission_content_allocations: loader
                .map_or(0, |loader| loader.content.live_allocation_count()),
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
    precision: Pc24PrecisionState,
    phase: MissionLoadPhase,
    entered: bool,
    scene: Option<SceneKey>,
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

    pub fn phase(&self) -> MissionLoadPhase {
        self.phase
    }
    pub fn startup_patches(&self) -> &ProgramPatchStartup {
        &self.startup_patches
    }
    pub fn failure(&self) -> Option<&MissionLoadFailure> {
        self.failure.as_ref()
    }
    /// Historical identity; after successful terminal disposal it is stale.
    pub fn scene(&self) -> Option<SceneKey> {
        self.scene
    }
    /// Retained CPU Close evidence for the actual Scene owner. The native
    /// device virtual remains outside this headless admission.
    pub fn scene_close_report(&self) -> Option<SceneCpuCloseReport> {
        self.scene
            .and_then(|scene| self.resources.cpu_close_report(scene))
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
                self.scene = Some(scene);
                scene
            }
            Err(failure) => {
                self.scene = failure.scene;
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
        self.discard.phase = MissionDiscardPhase::Scene;
        if let Some(scene) = self.scene {
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