//! Host composition of the existing CPU world intake and scene2 prefix loader.
//!
//! This is deliberately not C_mission::Open, MissionRuntime, GameInit, or an
//! actor/script scheduler. A completed admitted prefix does not establish a
//! playable mission. All scene, frame, texture and cache effects use the same
//! actual owners; a stopped load retains its first error and executed prefix.
use crate::c_str::StringError;
use crate::driver_scene::{
    DriverSceneContext, DriverSceneCreatePhase, DriverSceneError, DriverSceneOwner,
    DriverSceneScreen, NativeSceneSoundDriver,
};
use crate::game_camera_projection::GameCameraProjection;
use crate::host_scene_adapters::InstallationImages;
use crate::itexture::{CpuTextureStorage, TextureError, TextureResources};
use crate::lightmap_atlas::AtlasStatistics;
use crate::mission_resources::{
    CacheMemoryError, CpuMissionCacheMemory, MissionCacheError, MissionCacheKind, MissionCaches,
};
use crate::mission_scene_loading::{MissionSceneLoadError, MissionSceneLoader, SceneLoadMapPrefix};
use crate::model_loading::{CpuSceneWorldError, CpuSceneWorldProgress, ModelArena};
use crate::native_precision::Pc24PrecisionState;
use crate::scene_ownership::SceneKey;
use crate::scene_resources::{SceneResourceError, SceneResources};
use std::convert::Infallible;

/// Explicit probe inputs. Projection/sound state and cache directories are
/// supplied by the caller; this host adapter does not infer original GameInit.
pub struct MissionLoadRequest<'a> {
    /// Original ModelOpen input, such as `missions/example/scene.i3d`.
    /// The existing adapter performs the evidenced extension rewrite to 4DS.
    pub world_input: &'a str,
    pub scene2_path: &'a str,
    /// Exact AddDir inputs in caller order. Empty means genuinely no directory;
    /// it does not silently install a `Models` search path.
    pub model_directories: &'a [&'a [u8]],
    pub projection: GameCameraProjection,
    pub native_sound_driver: NativeSceneSoundDriver,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MissionLoadPhase {
    NotStarted,
    CacheDirectories,
    SceneCreation,
    World,
    LoadMap,
    Scene2Input,
    Scene2Prefix,
    /// Only the existing loader's admitted input completed. There is no
    /// transition to actor installation, gameplay, or mission completion.
    PrefixComplete,
}

#[derive(Debug)]
pub enum MissionLoadInitError {
    Textures(TextureError),
    CacheStrings(StringError),
}

#[derive(Debug)]
pub enum MissionLoadCause {
    CacheDirectory {
        index: usize,
        error: MissionCacheError<CacheMemoryError>,
    },
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
    WorldStaging,
    Cache(MissionCacheKind),
    Scene,
    DriverRegistry,
    RemainingOwners,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MissionDiscardProgress {
    pub phase: MissionDiscardPhase,
    /// Host disposal order is Sound, Model, Animation. This is not a claim
    /// about the original Game CloseSystem/global destruction sequence.
    pub caches_destructed: usize,
    pub scene_release_executed: bool,
    pub registry_storage_released: bool,
}
impl Default for MissionDiscardProgress {
    fn default() -> Self {
        Self {
            phase: MissionDiscardPhase::NotStarted,
            caches_destructed: 0,
            scene_release_executed: false,
            registry_storage_released: false,
        }
    }
}

/// Counts from actual owners, including partial or unregistered scene owners.
/// They are CPU ownership evidence, not native-device or original-runtime proof.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MissionOwnedCounts {
    pub live_frames: usize,
    pub driver_scenes: usize,
    pub driver_scene_capacity: u32,
    pub cache_allocations: usize,
    pub cache_bytes: usize,
    pub scene_graphics: usize,
    pub scene_pixels: usize,
    pub scene_strings: usize,
    pub scene_textures: usize,
    /// Actual registry in the scene texture owner's ObjectResources domain.
    pub scene_lightmaps: AtlasStatistics,
    pub visual_graphics: usize,
    pub visual_textures: usize,
    pub visual_allocations: usize,
    /// Includes pending/failed allocations, even if there is no ready atlas.
    pub visual_lightmaps: AtlasStatistics,
    pub sector_allocations: usize,
    pub portal_allocations: usize,
    pub occluder_allocations: usize,
    pub weather_references: i32,
}

#[derive(Debug)]
pub enum MissionDiscardCause {
    WorldStaging(DriverSceneError<Infallible>),
    Cache {
        kind: MissionCacheKind,
        error: MissionCacheError<CacheMemoryError>,
    },
    RegisteredScene(DriverSceneError<Infallible>),
    UnregisteredScene(SceneResourceError),
    DriverRegistry(DriverSceneError<Infallible>),
    RemainingOwners(Box<MissionOwnedCounts>),
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
}

impl MissionLoadSession {
    pub fn new(
        installation: lhr_assets::Installation,
        precision: Pc24PrecisionState,
    ) -> Result<Self, MissionLoadInitError> {
        Ok(Self {
            installation,
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
    pub fn failure(&self) -> Option<&MissionLoadFailure> {
        self.failure.as_ref()
    }
    /// Historical identity; after successful terminal disposal it is stale.
    pub fn scene(&self) -> Option<SceneKey> {
        self.scene
    }
    pub fn driver(&self) -> &DriverSceneOwner {
        &self.driver
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
    pub fn owned_counts(&self) -> MissionOwnedCounts {
        let visual = self.arena.loaded_visual_resources();
        let visual_backend = visual.map(|owner| owner.backend());
        MissionOwnedCounts {
            live_frames: self.arena.live_frame_count(),
            driver_scenes: self.driver.scenes().len(),
            driver_scene_capacity: self.driver.capacity(),
            cache_allocations: self.cache_memory.live_allocations(),
            cache_bytes: self.cache_memory.live_bytes(),
            scene_graphics: self.texture_storage.live_graphics(),
            scene_pixels: self.texture_storage.live_pixels(),
            scene_strings: self.texture_storage.live_strings(),
            scene_textures: self.textures.cached().len() + self.textures.uncached().len(),
            scene_lightmaps: self.textures.objects().lightmap_atlases().statistics(),
            visual_graphics: visual_backend.map_or(0, |owner| owner.live_graphics()),
            visual_textures: visual_backend.map_or(0, |owner| owner.live_textures()),
            visual_allocations: visual_backend.map_or(0, |owner| owner.live_allocations()),
            visual_lightmaps: visual.map_or_else(AtlasStatistics::default, |owner| {
                owner.objects().lightmap_atlases().statistics()
            }),
            sector_allocations: self.arena.sector_storage().live_allocations(),
            portal_allocations: self.arena.live_portal_allocations(),
            occluder_allocations: self.arena.occluder_storage().live_allocations(),
            weather_references: self.resources.ws_ref_count(),
        }
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
        self.phase = MissionLoadPhase::CacheDirectories;
        for (index, directory) in request.model_directories.iter().enumerate() {
            self.caches
                .add_dir(MissionCacheKind::Model, directory, &mut self.cache_memory)
                .map_err(|error| MissionLoadCause::CacheDirectory { index, error })?;
        }
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
            request.native_sound_driver,
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

        self.phase = MissionLoadPhase::LoadMap;
        match MissionSceneLoader::new(&self.arena, scene) {
            Ok(loader) => self.loader = Some(loader),
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
            .load_prefix_with_model_cache(
                &mut self.driver,
                &mut self.arena,
                &self.resources,
                screen,
                request.projection,
                self.scene2_bytes.as_deref().unwrap(),
                &mut self.caches,
                &self.installation,
                &mut self.cache_memory,
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
        self.discard.phase = MissionDiscardPhase::WorldStaging;
        if let Some(progress) = self.world.as_mut() {
            // This disposes only the actual temporary model. Already transferred
            // members remain owned by Scene until its subsequent release.
            self.driver
                .discard_scene_world_staging_cpu(&mut self.arena, &self.resources, progress)
                .map_err(MissionDiscardCause::WorldStaging)?;
        }
        for kind in [
            MissionCacheKind::Sound,
            MissionCacheKind::Model,
            MissionCacheKind::Animation,
        ] {
            self.discard.phase = MissionDiscardPhase::Cache(kind);
            self.caches
                .destruct(kind, &mut self.arena, &mut self.cache_memory)
                .map_err(|error| MissionDiscardCause::Cache { kind, error })?;
            self.discard.caches_destructed += 1;
        }

        self.discard.phase = MissionDiscardPhase::Scene;
        if let Some(scene) = self.scene {
            if self.driver.scenes().contains(&scene) {
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
        }
        self.discard.phase = MissionDiscardPhase::DriverRegistry;
        self.driver
            .release_empty_registry_storage()
            .map_err(MissionDiscardCause::DriverRegistry)?;
        self.discard.registry_storage_released = true;
        self.discard.phase = MissionDiscardPhase::RemainingOwners;
        let remaining = self.owned_counts();
        if remaining != MissionOwnedCounts::default() {
            return Err(MissionDiscardCause::RemainingOwners(Box::new(remaining)));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "mission_load_session_tests.rs"]
mod tests;
