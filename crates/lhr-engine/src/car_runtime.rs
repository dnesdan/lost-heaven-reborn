//! Standalone CPU car owner: construction, Init, optional GameInit/driving and
//! explicit destruction over shared frame/resource and driver-scene ownership.
//! Original mission population and player/camera scheduling remain separate.
use crate::{
    car_effects::CarTrackResources,
    loaded_visual_resources::MaterialLoadContext,
    model_loading::{ModelArena, ModelKey},
    native_precision::Pc24PrecisionState,
    scene_ownership::SceneKey,
    vehicle_bodycol::{VehicleBodycolScratch, VehicleBodycolStorage},
    vehicle_frame_backend::{VehicleFrameBackend, VehicleFrameMemory, VehicleMemoryError},
    vehicle_init::*,
};
#[path = "car_runtime_catalog.rs"]
mod catalog;
pub use catalog::CarInitAnimationCatalog;
#[path = "car_runtime_driving.rs"]
mod driving;
pub use driving::{CarDrivingControlFeedback, CarDrivingInput, CarDrivingState};
#[path = "car_runtime_game.rs"]
mod game;
#[path = "car_runtime_lifecycle.rs"]
mod lifecycle;
use lifecycle::OperationState;
#[path = "car_runtime_owner.rs"]
mod owner;
#[path = "car_runtime_scene.rs"]
mod scene_loading;
pub use owner::{failure_slots, inspect_failed_car, GuardedCarRuntime};
#[path = "car_runtime_tables.rs"]
mod tables;

const CAR: u32 = 0x10000;
const INIT_STACK: u32 = 0x800000;
const VEHICLE_STACK: u32 = INIT_STACK - 0x1d0;
const CONSTRUCTOR_STACK: u32 = 0x820000;
const GAME_STACK: u32 = 0x840000;

#[derive(Clone, Copy, Debug)]
pub struct CarRuntimeOptions {
    /// Explicit host allocation/caller-storage seed, not a native heap image.
    pub allocator_fill: u8,
    pub random_seed: u32,
    pub material_context: MaterialLoadContext,
    /// Host driver milliseconds before any material constructor/load restart.
    pub material_driver_time: i32,
    /// Host CPU inspection screen used by the Scene constructor queries.
    pub scene_screen_size: [i32; 2],
    /// IGraph viewport boundaries; None uses the full host inspection screen.
    pub scene_viewport: Option<[i32; 4]>,
}
impl Default for CarRuntimeOptions {
    fn default() -> Self {
        Self {
            allocator_fill: 0,
            scene_screen_size: [640, 480],
            scene_viewport: None,
            random_seed: 1,
            material_context: MaterialLoadContext::ORIGINAL_DRIVER_INIT_DEFAULTS,
            material_driver_time: 0,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CarRuntimeSummary {
    pub constructed: bool,
    pub initialized: bool,
    pub seats: usize,
    pub cached_tracks: usize,
    pub source_frames: usize,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CarSteeringPreviewState {
    pub maximum_angle: f32,
    pub requested_angle: f32,
    pub applied_angle: f32,
    pub wheels: usize,
    pub steerable_wheels: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CarRuntimeLevelTableSummary {
    /// Original loader success byte. A parsed table can contain no match.
    pub loaded: bool,
    pub records: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CarRuntimeCleanup {
    pub actor_count: u32,
    pub actor_registry_entries: usize,
    pub heap_allocations: usize,
    pub graphics: usize,
    pub textures: usize,
    pub cpu_allocations: usize,
    pub audio_sounds: usize,
    pub audio_voices: usize,
    pub audio_sources: usize,
    pub audio_listener_pose: bool,
    pub driver_sound_frames: usize,
    pub driver_listeners: usize,
    /// Live scene/sector frames after the admitted CPU driver cleanup.
    pub retained_scene_prefix_frames: usize,
    pub driver_scene_registrations: usize,
    pub driver_scene_capacity: u32,
    /// CPU projection: original DeviceVirtual14 remains unexecuted/uncredited.
    pub scene_cpu_close: Option<crate::scene_resources::SceneCpuCloseReport>,
}
pub struct CarRuntimeOpenFailure {
    pub message: String,
    pub partial: Box<CarRuntime>,
}
impl std::fmt::Display for CarRuntimeOpenFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(f)
    }
}

pub struct CarRuntime {
    precision: Pc24PrecisionState,
    services: driving::CarRuntimeServices,
    driving: Option<driving::DrivingSession>,
    game_init: OperationState,
    game_done: OperationState,
    game: game::CarRuntimeGameState,
    game_scratch: CarGameInitScratch,
    level_table_started: bool,
    installation: lhr_assets::Installation,
    arena: ModelArena,
    memory: VehicleFrameMemory,
    tracks: CarTrackResources,
    model: Option<ModelKey>,
    scene: Option<SceneKey>,
    scene_world_progress: Option<crate::model_loading::CpuSceneWorldProgress>,
    scene_city_cache: crate::canonical_city_cache::CanonicalCityCache,
    scene_cache_bucket: Option<[i32; 2]>,
    scene_mission: Option<scene_loading::DrivingSceneMission>,
    scene_resources: crate::scene_resources::SceneResources,
    scene_driver: crate::driver_scene::DriverSceneOwner,
    scene_textures: Option<crate::itexture::TextureResources>,
    scene_storage: crate::itexture::CpuTextureStorage,
    construction: Option<CarConstructionState>,
    car: Option<VehicleInitState>,
    summary: CarRuntimeSummary,
    car_destruction: OperationState,
    shutdown_started: bool,
    wheel_preview_started: bool,
    wheel_preview_failed: bool,
    // Last field: even retained-runtime destruction drops source owners first.
    startup_patches: Option<crate::program_patch::ProgramPatchLease>,
}
struct ConstructorBackend<'a>(&'a mut VehicleFrameMemory);
impl CarConstructionBackend for ConstructorBackend<'_> {
    type Error = VehicleMemoryError;
    fn read_word(&mut self, address: u32) -> Result<u32, Self::Error> {
        VehicleInitBackend::read_word(self.0, address)
    }
    fn write_word(&mut self, address: u32, value: u32) -> Result<(), Self::Error> {
        VehicleInitBackend::write_word(self.0, address, value)
    }
    fn allocate_car_construction(
        &mut self,
        _: CarConstructionMemory<'_>,
        bytes: u32,
    ) -> Result<u32, Self::Error> {
        VehicleInitBackend::allocate(self.0, bytes)
    }
    fn free_car_construction(
        &mut self,
        _: CarConstructionMemory<'_>,
        allocation: u32,
    ) -> Result<(), Self::Error> {
        VehicleInitBackend::free(self.0, allocation)
    }
}
impl CarRuntime {
    /// Requires only an original asset installation and reconstructed semantic
    /// constants. No native constructor bytes, executable or probe are read.
    pub fn open(
        installation: lhr_assets::Installation,
        path: &str,
        options: CarRuntimeOptions,
    ) -> Result<Self, CarRuntimeOpenFailure> {
        Self::open_context(installation, path, options, None, false, None)
    }
    /// Establishes the authored model transform before construction and Init.
    pub fn open_driving(
        installation: lhr_assets::Installation,
        path: &str,
        options: CarRuntimeOptions,
        mission: &str,
    ) -> Result<Self, CarRuntimeOpenFailure> {
        Self::open_context(installation, path, options, Some(mission), false, None)
    }
    /// Driving inspection with the original static world owned by the same
    /// Scene as the car. This does not imply original MissionOpen scheduling.
    pub fn open_driving_scene(
        installation: lhr_assets::Installation,
        path: &str,
        options: CarRuntimeOptions,
        mission: &str,
    ) -> Result<Self, CarRuntimeOpenFailure> {
        Self::open_context(installation, path, options, Some(mission), true, None)
    }
    /// Production driving borrows startup data and retains its actual owner
    /// through initialization, source loading and any interrupted cleanup.
    pub fn open_driving_scene_with_patches(
        installation: lhr_assets::Installation,
        path: &str,
        options: CarRuntimeOptions,
        mission: &str,
        patches: &crate::program_patch::ProgramPatchLease,
    ) -> Result<Self, CarRuntimeOpenFailure> {
        Self::open_context(
            installation,
            path,
            options,
            Some(mission),
            true,
            Some(patches.clone()),
        )
    }
    fn open_context(
        installation: lhr_assets::Installation,
        path: &str,
        options: CarRuntimeOptions,
        mission: Option<&str>,
        canonical_world: bool,
        startup_patches: Option<crate::program_patch::ProgramPatchLease>,
    ) -> Result<Self, CarRuntimeOpenFailure> {
        let tracks = CarInitAnimationCatalog::resources().expect("bounded unique semantic catalog");
        let mut runtime = Self {
            startup_patches,
            precision: Pc24PrecisionState::default(),
            services: driving::CarRuntimeServices {
                allocator_fill: options.allocator_fill,
                ..Default::default()
            },
            driving: None,
            game_init: OperationState::NotStarted,
            game_done: OperationState::NotStarted,
            game: game::CarRuntimeGameState::default(),
            game_scratch: CarGameInitScratch {
                memory: VehicleComponentTailScratch {
                    address: GAME_STACK - 0x120,
                    bytes: [options.allocator_fill; VEHICLE_INIT_STACK_BYTES],
                },
                stack: GAME_STACK,
                registers: CarInitRegisters {
                    ebx: 0,
                    ebp: 0,
                    esi: 0,
                    edi: 0,
                },
            },
            level_table_started: false,
            installation,
            arena: ModelArena::default(),
            memory: VehicleFrameMemory::default(),
            tracks,
            model: None,
            scene: None,
            scene_world_progress: None,
            scene_city_cache: Default::default(),
            scene_cache_bucket: None,
            scene_mission: None,
            scene_resources: Default::default(),
            scene_driver: Default::default(),
            scene_textures: None,
            scene_storage: Default::default(),
            construction: None,
            car: None,
            summary: CarRuntimeSummary {
                constructed: false,
                initialized: false,
                seats: 0,
                cached_tracks: 0,
                source_frames: 0,
            },
            car_destruction: OperationState::NotStarted,
            shutdown_started: false,
            wheel_preview_started: false,
            wheel_preview_failed: false,
        };
        let initialized = runtime
            .arena
            .set_material_driver_time(options.material_driver_time)
            .map_err(|e| e.to_string())
            .and_then(|()| runtime.initialize(path, options, mission, canonical_world));
        if let Err(message) = initialized {
            return Err(CarRuntimeOpenFailure {
                message,
                partial: Box::new(runtime),
            });
        }
        Ok(runtime)
    }
    fn initialize(
        &mut self,
        path: &str,
        options: CarRuntimeOptions,
        mission: Option<&str>,
        canonical_world: bool,
    ) -> Result<(), String> {
        Pc24PrecisionState::run_owner(
            self,
            |owner| &mut owner.precision,
            |owner| owner.initialize_inner(path, options, mission, canonical_world),
        )
    }
    fn initialize_inner(
        &mut self,
        path: &str,
        options: CarRuntimeOptions,
        mission: Option<&str>,
        canonical_world: bool,
    ) -> Result<(), String> {
        self.prepare_game_context(options, mission.is_some())?;
        if let Some(mission) = mission {
            self.prepare_driving_world(path, mission)?;
        }
        self.arena
            .visual_resource_owner_mut()
            .map_err(|e| e.to_string())?
            .set_material_load_context(options.material_context);
        self.arena
            .visual_resource_owner_mut()
            .map_err(|e| e.to_string())?
            .enable_inspection_rgba8_color_key()
            .map_err(|e| format!("Car inspection format: {e:?}"))?;
        if self.scene_textures.is_none() {
            self.scene_textures = Some(
                crate::itexture::TextureResources::new()
                    .map_err(|error| format!("Scene texture owner: {error:?}"))?,
            );
        }
        let [width, height] = options.scene_screen_size;
        // The canonical driving Scene and the vehicle effects use the same
        // Arc-backed PCM mixer. Bind it before CreateListener/Scene creation.
        // Model-only car inspection keeps its existing no-sound Scene path.
        let scene_sound_driver = if canonical_world {
            self.scene_driver
                .bind_host_sound_backend(&mut self.arena, &self.services.audio)
                .map_err(|error| format!("Driving host PCM backend: {error:?}"))?;
            crate::driver_scene::NativeSceneSoundDriver::Initialized
        } else {
            crate::driver_scene::NativeSceneSoundDriver::Absent
        };
        let created = self.scene_driver.create_scene(
            &mut crate::driver_scene::DriverSceneContext {
                arena: &mut self.arena,
                resources: &mut self.scene_resources,
                textures: self.scene_textures.as_mut().unwrap(),
                storage: &mut self.scene_storage,
            },
            &mut InspectionScreen {
                size: options.scene_screen_size,
                viewport: options.scene_viewport.unwrap_or([0, 0, width, height]),
            },
            &mut InstallationImages(&self.installation),
            scene_sound_driver,
        );
        let scene = match created {
            Ok(scene) => {
                self.scene = Some(scene);
                scene
            }
            Err(failure) => {
                self.scene = failure.scene;
                return Err(format!("Driver scene creation: {failure:?}"));
            }
        };
        if canonical_world {
            let mission = mission.ok_or("Canonical driving world requires a mission source")?;
            let name = format!("missions\\{mission}\\scene.i3d");
            match self.scene_driver.load_scene_world_cpu(
                &mut self.arena,
                &self.scene_resources,
                scene,
                &self.installation,
                &name,
            ) {
                Ok(progress) => self.scene_world_progress = Some(progress),
                Err(failure) => {
                    self.scene_world_progress = Some(failure.progress);
                    return Err(format!("Canonical driving world: {:?}", failure.error));
                }
            }
            self.load_driving_scene2(mission, options)?;
        }
        let sector = self.arena.scene(scene).map_err(|e| e.to_string())?.sectors[0].frame();
        let model = self.arena.create_model().map_err(|e| e.to_string())?;
        self.model = Some(model);
        let input = original_model_input(path)?;
        let instance = path
            .rsplit(['/', '\\'])
            .next()
            .and_then(|name| name.rsplit_once('.').map(|(stem, _)| stem))
            .filter(|name| !name.is_empty())
            .ok_or("Car inspection model has no instance name")?;
        self.arena
            .set_frame_name(model.root(), Some(instance.as_bytes()))
            .map_err(|e| e.to_string())?;
        let content = self
            .arena
            .open_original_static_model(model, &self.installation, &input)
            .map_err(|failure| format!("Car model open: {}", failure.error))?;
        self.summary.source_frames = content.frames().len();
        self.arena
            .scene_add_frame(scene, model.root())
            .map_err(|e| e.to_string())?;
        self.arena
            .link_frame(model.root(), Some(sector), 0)
            .map_err(|e| e.to_string())?;
        if let Some(driving) = self.driving.as_ref() {
            self.arena
                .graph
                .assign_position(model.root(), driving.spawn.position)
                .map_err(|e| format!("Driving spawn position: {e:?}"))?;
            self.arena
                .graph
                .assign_rotation(
                    model.root(),
                    crate::native_transform::SQuat {
                        wxyz: driving.spawn.rotation_wxyz,
                    },
                )
                .map_err(|e| format!("Driving spawn rotation: {e:?}"))?;
            self.arena
                .graph
                .assign_scale(model.root(), driving.spawn.scale)
                .map_err(|e| format!("Driving spawn scale: {e:?}"))?;
            self.arena
                .graph
                .update(model.root())
                .map_err(|e| format!("Driving spawn publication: {e:?}"))?;
        }
        let registers = CarInitRegisters {
            ebx: 0,
            ebp: 0,
            esi: 0,
            edi: 0,
        };
        let mut init_scratch = CarInitScratch {
            address: INIT_STACK,
            bytes: [options.allocator_fill; CAR_INIT_STACK_BYTES],
        };
        let model_address = {
            let backend = VehicleFrameBackend::new(&mut self.arena, &mut self.memory);
            backend
                .frame_address(model.root())
                .map_err(|e| format!("Car model address: {e:?}"))?
        };
        init_scratch.bytes[0x26c..0x270].copy_from_slice(&model_address.to_le_bytes());
        let constructor = CarConstructionState::from_allocator_bytes(
            CAR,
            vec![options.allocator_fill; 0x221c],
            CarConstructionScratch {
                address: CONSTRUCTOR_STACK,
                bytes: [options.allocator_fill; CAR_CONSTRUCTION_STACK_BYTES],
            },
            CarConstructionRegisters {
                ebx: 0,
                ebp: 0,
                esi: 0,
                edi: 0,
                seh: 0,
            },
            init_scratch,
            registers,
        )
        .map_err(|e| format!("Car constructor storage: {e:?}"))?;
        self.construction = Some(constructor);
        self.construction
            .as_mut()
            .unwrap()
            .construct(&mut ConstructorBackend(&mut self.memory))
            .map_err(|e| format!("Car construction: {e:?}"))?;
        let constructor = self.construction.take().unwrap();
        self.car = Some(constructor.into_car().map_err(|state| {
            self.construction = Some(state);
            "Car constructor did not complete".to_owned()
        })?);
        self.summary.constructed = true;
        let constructed = self.car.as_ref().unwrap().car_bytes().unwrap();
        if u32::from_le_bytes(constructed[..4].try_into().unwrap()) != 0x623618 {
            return Err("Car constructor produced an unsupported virtual Init table".into());
        }
        let storage = VehicleComponentTailStorage {
            bodycol: VehicleBodycolStorage {
                vehicle_address: CAR + 0x70,
                callback_scratch: VehicleBodycolScratch {
                    record: 0x600000,
                    leading_byte: options.allocator_fill,
                    nested_byte: options.allocator_fill,
                },
                descendant_scratch: 0x600100,
            },
            scratch: VehicleComponentTailScratch {
                address: VEHICLE_STACK,
                bytes: [options.allocator_fill; VEHICLE_INIT_STACK_BYTES],
            },
        };
        let outcome = {
            let mut backend = VehicleFrameBackend::with_installation(
                &mut self.arena,
                &mut self.memory,
                &self.installation,
            )
            .with_car_track_resources(&mut self.tracks);
            self.car
                .as_mut()
                .unwrap()
                .init_car(&mut backend, model_address, storage)
                .map_err(|e| format!("Car virtual Init: {e:?}"))?
        };
        if outcome != CarInitOutcome::Initialized {
            return Err(format!("Car virtual Init rejected this model: {outcome:?}"));
        }
        self.summary.initialized = true;
        let bytes = self.car.as_ref().unwrap().car_bytes().unwrap();
        let begin = u32::from_le_bytes(bytes[0x21cc..0x21d0].try_into().unwrap());
        let end = u32::from_le_bytes(bytes[0x21d0..0x21d4].try_into().unwrap());
        self.summary.seats = if begin == 0 {
            0
        } else {
            end.checked_sub(begin)
                .ok_or("Invalid initialized seat extent")? as usize
                / 160
        };
        self.summary.cached_tracks = self.tracks.cached_tracks().count();
        Ok(())
    }
    fn prepare_game_context(
        &mut self,
        options: CarRuntimeOptions,
        driving: bool,
    ) -> Result<(), String> {
        let fill = options.allocator_fill;
        let regions: Vec<(u32, Vec<u8>)> = vec![
            // Caller storage below the canonical GameInit scratch. The level
            // loader's original large stack frame must not alias a second
            // copy of that scratch or of the live car.
            (GAME_STACK - 0x2400, vec![fill; 0x22e0]),
            (0x600000, vec![fill; 0x200]),
            (INIT_STACK - 0x10000, vec![fill; 0xfe30]),
            (INIT_STACK - 8, vec![fill; 8]),
            (0x63788c, 0x647df8u32.to_le_bytes().to_vec()),
            (0x647df8, vec![0; 0x100]),
            (0x63d890, vec![0; 16]),
            (0x63d9c8, vec![0; 8]),
            (0x6d87c4, vec![0; 4]),
            (0x63a4cc, 1u32.to_le_bytes().to_vec()),
            (0x6c3e1c, vec![0; 8]),
            (0x647908, vec![0; 8]),
            (0x647918, vec![0; 8]),
            (0x647998, vec![0; 8]),
            // This standalone caller has no original player/camera actors.
            (0x647f28, vec![0; 8]),
            (0x63d2bc, vec![0; 16]),
            (0x63a200, options.random_seed.to_le_bytes().to_vec()),
        ];
        for (address, bytes) in regions {
            self.memory
                .map_bytes(address, &bytes)
                .map_err(|e| format!("Game inspection context: {e:?}"))?;
        }
        if driving {
            self.memory
                .map_bytes(0x870000, &[fill; 24])
                .map_err(|e| format!("Game spatial sound caller stack: {e:?}"))?;
            // Primary PC literal used by C_car::Break_Glass (4262f0).
            self.memory
                .map_bytes(0x6323b8, b"lustr1.wav\0")
                .map_err(|e| format!("Glass sound literal: {e:?}"))?;
            self.memory
                .map_bytes(0x63d6d4, &[0; 4])
                .map_err(|e| format!("Crash actor counter context: {e:?}"))?;
            self.memory
                .map_bytes(0x63d2a4, &[0; 16])
                .map_err(|e| format!("Game frame-pair context: {e:?}"))?;
            // Original BSS containers and their C_game startup resets:
            // 531130 sets the first manager's selector to -1; the remaining
            // empty-vector branches (532da0/534ec0) retain zero extents.
            let mut managers = [0u8; 76];
            managers[..4].copy_from_slice(&u32::MAX.to_le_bytes());
            self.memory
                .map_bytes(0x646d44, &managers)
                .map_err(|e| format!("Game actor manager context: {e:?}"))?;
            let mut sound_manager = [0u8; 28];
            sound_manager[..4].copy_from_slice(&0x625410u32.to_le_bytes());
            self.memory
                .map_bytes(0x646f50, &sound_manager)
                .map_err(|e| format!("Indexed sound manager context: {e:?}"))?;
            // Original model-cache startup state (5bd080), including the
            // Models directory registered by the game caller. These pointers
            // own actual caller allocations rather than borrowed file data.
            let mut cache = [0u8; 40];
            cache[..4].copy_from_slice(&0x625cf4u32.to_le_bytes());
            cache[36..40].copy_from_slice(&256u32.to_le_bytes());
            self.memory
                .map_bytes(0x647dd0, &cache)
                .map_err(|e| format!("Model cache context: {e:?}"))?;
            let mut directory = [0u8; 16];
            directory[..4].copy_from_slice(&1u32.to_le_bytes());
            directory[4..8].copy_from_slice(&7u32.to_le_bytes());
            directory[8..].copy_from_slice(b"Models\\\0");
            let directory = self.own_game_table(&directory)?;
            let vector = self.own_game_table(&directory.to_le_bytes())?;
            for (address, value) in [
                (0x647de8, vector),
                (0x647dec, vector + 4),
                (0x647df0, vector + 4),
            ] {
                VehicleInitBackend::write_word(&mut self.memory, address, value)
                    .map_err(|e| format!("Model cache directory: {e:?}"))?;
            }
        }
        Ok(())
    }
    pub fn summary(&self) -> &CarRuntimeSummary {
        &self.summary
    }
    pub fn arena(&self) -> &ModelArena {
        &self.arena
    }
    pub fn arena_mut(&mut self) -> &mut ModelArena {
        &mut self.arena
    }
    pub fn scene(&self) -> Option<SceneKey> {
        self.scene
    }
    pub fn scene_viewport(&self) -> Option<crate::scene_resources::SceneViewport> {
        self.scene
            .and_then(|scene| self.scene_resources.construction(scene))
            .and_then(|construction| construction.viewport_projection)
    }
    pub fn has_canonical_scene_world(&self) -> bool {
        self.scene_world_progress.as_ref().is_some_and(|progress| {
            progress.phase() == crate::model_loading::CpuSceneWorldPhase::Complete
        })
    }
    pub fn scene_city_cache(&self) -> &crate::canonical_city_cache::CanonicalCityCache {
        &self.scene_city_cache
    }
    pub fn scene2_frontier(&self) -> Option<&crate::mission_scene_loading::MissionSceneLoadError> {
        self.scene_mission
            .as_ref()
            .and_then(|mission| mission.frontier.as_ref())
    }
    pub fn set_scene_viewport(&mut self, rectangle: [i32; 4]) -> Result<(), String> {
        let scene = self.scene.ok_or("Car scene owner unavailable")?;
        self.scene_resources
            .set_viewport(&self.arena, scene, rectangle)
            .map_err(|error| format!("Scene viewport: {error:?}"))?;
        Ok(())
    }
    pub fn driver_scene_registry(&self) -> &crate::driver_scene::DriverSceneOwner {
        &self.scene_driver
    }
    pub fn model(&self) -> Option<ModelKey> {
        self.model
    }
    pub fn cached_track_count(&self) -> usize {
        self.tracks.cached_tracks().count()
    }

    /// Runs the original LoadLevelTables against this car's initialized name
    /// and the installation's vehicles.bin. Records remain owned by this car
    /// and are released by its original destructor. This does not select a
    /// level, invoke GameInit, or start simulation. A failed call is retained
    /// for explicit cleanup rather than replayed.
    pub fn load_level_table(&mut self) -> Result<CarRuntimeLevelTableSummary, String> {
        Pc24PrecisionState::run_owner(
            self,
            |owner| &mut owner.precision,
            |owner| owner.load_level_table_inner(),
        )
    }
    fn load_level_table_inner(&mut self) -> Result<CarRuntimeLevelTableSummary, String> {
        if !self.car_is_live() || self.level_table_started {
            return Err(
                "Level tables require a live initialized car and a fresh table call".into(),
            );
        }
        self.level_table_started = true;
        let car = self
            .car
            .as_mut()
            .ok_or("Level table has no initialized car")?;
        let mut frames = VehicleFrameBackend::with_installation(
            &mut self.arena,
            &mut self.memory,
            &self.installation,
        )
        .with_game_init_stack(GAME_STACK);
        let mut backend = game::CarRuntimeGameBackend::new(
            &mut frames,
            &self.installation,
            &mut self.game,
            GAME_STACK,
        );
        let mut memory = car
            .car_live_memory(&mut self.game_scratch.memory)
            .ok_or("Level table has no canonical car storage")?;
        let result = backend
            .game_init_effect(
                memory.reborrow(),
                CarGameInitEffect::CarDependency {
                    entry: 0x41e270,
                    car: CAR,
                    argument: 0,
                },
            )
            .map_err(|e| format!("Car level table: {e}"))?;
        let begin = memory
            .read_word_with_backend(&mut backend, CAR + 0x21b4)
            .map_err(|e| format!("Car level table begin: {e}"))?;
        let end = memory
            .read_word_with_backend(&mut backend, CAR + 0x21b8)
            .map_err(|e| format!("Car level table end: {e}"))?;
        let bytes = end
            .checked_sub(begin)
            .filter(|n| n.is_multiple_of(0x113c))
            .ok_or("Car level table produced an invalid record extent")?;
        if self.game.open_handles() != 0 {
            return Err("Car level table retained an open original file handle".into());
        }
        Ok(CarRuntimeLevelTableSummary {
            loaded: result as u8 != 0,
            records: bytes as usize / 0x113c,
        })
    }

    /// Read-only control diagnostics from the canonical vehicle and wheel heap.
    pub fn steering_preview_state(&mut self) -> Result<CarSteeringPreviewState, String> {
        let bytes = self
            .car
            .as_ref()
            .ok_or("Car is not constructed")?
            .as_bytes();
        let word = |offset: usize| -> Result<u32, String> {
            Ok(u32::from_le_bytes(
                bytes
                    .get(offset..offset + 4)
                    .ok_or("Incomplete steering state")?
                    .try_into()
                    .unwrap(),
            ))
        };
        let count = word(0x4c4)? as usize;
        if count > 1024 {
            return Err("Wheel diagnostics exceed their storage budget".into());
        }
        let order = word(0xc38)?;
        let mut steerable = 0;
        for i in 0..count {
            let wheel =
                VehicleInitBackend::read_word(&mut self.memory, order.wrapping_add(i as u32 * 4))
                    .map_err(|e| format!("Wheel diagnostics: {e:?}"))?;
            let flags = VehicleInitBackend::read_byte(&mut self.memory, wheel.wrapping_add(0x120))
                .map_err(|e| format!("Wheel diagnostics: {e:?}"))?;
            steerable += usize::from(flags & 1 != 0);
        }
        Ok(CarSteeringPreviewState {
            maximum_angle: f32::from_bits(word(0x610)?),
            requested_angle: f32::from_bits(word(0x624)?),
            applied_angle: f32::from_bits(word(0x628)?),
            wheels: count,
            steerable_wheels: steerable,
        })
    }

    /// Host wheel preview on the same initialized car. It calls the original
    /// SetSteer and UpdateSteeringWheels; it does not run GameInit or dynamics.
    /// A zero-time first update establishes the original speed-dependent steer
    /// limit before the first input. Later calls retain smoothing state.
    pub fn preview_wheel_steering(&mut self, input: f32, dt_seconds: f32) -> Result<(), String> {
        Pc24PrecisionState::run_owner(
            self,
            |owner| &mut owner.precision,
            |owner| owner.preview_wheel_steering_inner(input, dt_seconds),
        )
    }
    fn preview_wheel_steering_inner(&mut self, input: f32, dt_seconds: f32) -> Result<(), String> {
        if !input.is_finite()
            || !(-1.0..=1.0).contains(&input)
            || !dt_seconds.is_finite()
            || !(0.0..=83.0_f32 * 0.001_f32).contains(&dt_seconds)
        {
            return Err(
                "Wheel preview requires steer in [-1,1] and elapsed seconds in [0,0.083]".into(),
            );
        }
        if !self.car_is_live() || self.wheel_preview_failed {
            return Err(
                "Wheel preview requires a live initialized car without a previous preview failure"
                    .into(),
            );
        }
        self.wheel_preview_failed = true;
        let car = self
            .car
            .as_mut()
            .ok_or("Wheel preview has no initialized car")?;
        let mut backend = VehicleFrameBackend::new(&mut self.arena, &mut self.memory);
        if !self.wheel_preview_started {
            car.wheel_pose()
                .update_steering_wheels(0.0, &mut backend)
                .map_err(|e| format!("Initial wheel steering update: {e:?}"))?;
            self.wheel_preview_started = true;
        }
        if !car
            .controls()
            .set_steer(input)
            .map_err(|e| format!("SetSteer: {e:?}"))?
        {
            return Err("Original SetSteer rejected the wheel preview input".into());
        }
        car.wheel_pose()
            .update_steering_wheels(dt_seconds, &mut backend)
            .map_err(|e| format!("Wheel steering update: {e:?}"))?;
        let model = self.model.ok_or("Wheel preview has no initialized model")?;
        self.arena
            .graph
            .update(model.root())
            .map_err(|e| format!("Wheel frame publication: {e:?}"))?;
        self.wheel_preview_failed = false;
        Ok(())
    }
    /// Runs only C_car destruction. Shared Game track metadata and actor-vector
    /// capacity remain owned until the later explicit Game-context shutdown.
    pub fn destruct_car(&mut self) -> Result<(), String> {
        Pc24PrecisionState::run_owner(
            self,
            |owner| &mut owner.precision,
            |owner| owner.destruct_car_inner(),
        )
    }
    fn destruct_car_inner(&mut self) -> Result<(), String> {
        self.cleanup_driving_actors_inner()?;
        if self.game_init.has_started() && !self.game_done.is_complete() {
            self.finish_game()?;
        }
        if self.car_destruction.has_started() {
            return Err("Car destruction already started".into());
        }
        self.car_destruction = OperationState::Started;
        if let Some(car) = self.car.as_mut() {
            let mut backend = VehicleFrameBackend::with_installation(
                &mut self.arena,
                &mut self.memory,
                &self.installation,
            )
            .with_car_track_resources(&mut self.tracks);
            car.destruct_car(&mut backend)
                .map_err(|e| format!("Car destruction: {e:?}"))?;
        }
        self.car_destruction = OperationState::Complete;
        Ok(())
    }
    /// Explicit cleanup after the renderer has finished using its uploads.
    /// Failure preserves the completed prefix and cannot replay destruction.
    pub fn shutdown(&mut self) -> Result<CarRuntimeCleanup, String> {
        if self.shutdown_started {
            return Err("Car inspection cleanup already started".into());
        }
        self.shutdown_started = true;
        if self.construction.is_some() {
            return Err(
                "Incomplete car constructor retained; derived destruction is unavailable".into(),
            );
        }
        if !self.car_destruction.is_complete() {
            self.destruct_car()?;
        }
        let model_to_check = self.model;
        if let Some(model) = self.model {
            let scene_owned = self.scene.is_some_and(|scene| {
                self.arena
                    .frame(model.root())
                    .is_ok_and(|frame| frame.owner == Some(scene.into()))
            });
            if scene_owned {
                // Transfer the last lifecycle work to native Close's owned-frame
                // stage. Drop only this caller's construction reference here;
                // the scene reference keeps the model/resources live for Unload.
                if self
                    .arena
                    .frame(model.root())
                    .map_err(|error| error.to_string())?
                    .references
                    < 2
                {
                    return Err("Car scene model lost the caller's owned reference".into());
                }
                self.arena
                    .release(model.root())
                    .map_err(|error| error.to_string())?;
            } else {
                self.arena
                    .link_frame(model.root(), None, 0)
                    .map_err(|error| error.to_string())?;
                self.arena.close(model).map_err(|error| error.to_string())?;
                self.arena
                    .release(model.root())
                    .map_err(|error| error.to_string())?;
            }
            self.model = None;
        }
        if self.scene.is_none() {
            self.arena
                .reclaim_unused_visual_vertices()
                .map_err(|error| error.to_string())?;
            self.arena
                .reclaim_unused_visual_indices()
                .map_err(|error| error.to_string())?;
        }
        let begin = VehicleInitBackend::read_word(&mut self.memory, 0x63d2bc)
            .map_err(|e| format!("Actor registry: {e:?}"))?;
        let end = VehicleInitBackend::read_word(&mut self.memory, 0x63d2c0)
            .map_err(|e| format!("Actor registry: {e:?}"))?;
        let actor_count = VehicleInitBackend::read_word(&mut self.memory, 0x63d2c8)
            .map_err(|e| format!("Actor registry: {e:?}"))?;
        let entries = end
            .checked_sub(begin)
            .ok_or("Invalid actor registry extent")? as usize
            / 4;
        if actor_count != 0 || entries != 0 {
            return Err("Car cleanup retained an actor registration".into());
        }
        VehicleInitBackend::free(&mut self.memory, begin)
            .map_err(|e| format!("Actor registry release: {e:?}"))?;
        for address in [0x63d2bc, 0x63d2c0, 0x63d2c4] {
            VehicleInitBackend::write_word(&mut self.memory, address, 0)
                .map_err(|e| format!("Actor registry reset: {e:?}"))?;
        }
        if self.game_init.has_started() {
            // C_car removes its entries; the standalone caller still owns the
            // C_using_object vector capacity in its game context.
            let header = 0x680000 + 0x29ec;
            let begin = VehicleInitBackend::read_word(&mut self.memory, header + 4)
                .map_err(|e| format!("Using-object registry: {e:?}"))?;
            let end = VehicleInitBackend::read_word(&mut self.memory, header + 8)
                .map_err(|e| format!("Using-object registry: {e:?}"))?;
            if begin != end {
                return Err("Car cleanup retained a using-object registration".into());
            }
            VehicleInitBackend::free(&mut self.memory, begin)
                .map_err(|e| format!("Using-object capacity release: {e:?}"))?;
            for offset in [4, 8, 12] {
                VehicleInitBackend::write_word(&mut self.memory, header + offset, 0)
                    .map_err(|e| format!("Using-object registry reset: {e:?}"))?;
            }
            // CreateCrashModel retains borrowed world identities here. The
            // standalone game caller owns only the pointer-vector capacity;
            // world model references remain with the world graph.
            let header = 0x680000 + 0x10;
            let begin = VehicleInitBackend::read_word(&mut self.memory, header + 4)
                .map_err(|e| format!("Hidden world models: {e:?}"))?;
            VehicleInitBackend::free(&mut self.memory, begin)
                .map_err(|e| format!("Hidden world model capacity release: {e:?}"))?;
            for offset in [4, 8, 12] {
                VehicleInitBackend::write_word(&mut self.memory, header + offset, 0)
                    .map_err(|e| format!("Hidden world model registry reset: {e:?}"))?;
            }
        }
        if self.game_init.has_started() {
            if let Some(session) = self.driving.as_mut() {
                session.simulation.cleanup(&mut self.memory)?;
                self.memory
                    .map_bytes(0x860000, &[self.services.allocator_fill; 16])
                    .map_err(|e| format!("Driving world cleanup caller: {e:?}"))?;
                crate::dynamic_collision_manager::release_owned_dynamic_grid(
                    &mut self.memory,
                    0x647f48,
                    0x860000,
                )
                .map_err(|e| format!("Driving world cleanup: {e:?}"))?;
            }
        }
        self.release_driving_tables()?;
        self.tracks.reset_track_cache();
        self.game.release_host_handles();
        let mut scene_cpu_close = None;
        self.scene_city_cache.close(&mut self.arena)?;
        if let Some(mission) = self.scene_mission.as_mut() {
            mission
                .loader
                .release_pending_visual_replacement(&mut self.arena)
                .map_err(|error| format!("Driving pending visual cleanup: {error:?}"))?;
        }
        if let Some(progress) = self.scene_world_progress.as_mut() {
            self.scene_driver
                .discard_scene_world_staging_cpu(&mut self.arena, &self.scene_resources, progress)
                .map_err(|error| format!("Canonical driving world staging cleanup: {error:?}"))?;
        }
        if let Some(scene) = self.scene {
            let textures = self
                .scene_textures
                .as_mut()
                .ok_or("Scene texture owner unavailable")?;
            if self.scene_driver.scenes().contains(&scene) {
                self.scene_driver
                    .release_scene(
                        &mut crate::driver_scene::DriverSceneContext {
                            arena: &mut self.arena,
                            resources: &mut self.scene_resources,
                            textures,
                            storage: &mut self.scene_storage,
                        },
                        scene,
                    )
                    .map_err(|error| format!("Driver scene destruction: {error:?}"))?;
            } else {
                // An unsupported caller dependency can leave the completed
                // constructor prefix unregistered. Explicit cleanup owns that
                // prefix; creation never rolls it back automatically.
                self.scene_resources
                    .release_scene(&mut self.arena, scene, textures, &mut self.scene_storage)
                    .map_err(|error| format!("Unregistered scene prefix cleanup: {error:?}"))?;
                self.scene_driver
                    .release_unregistered_scene_listener(scene)
                    .map_err(|error| format!("Unregistered scene listener cleanup: {error:?}"))?;
            }
            if self.arena.scene(scene).is_ok() {
                return Err("Driver scene retained an unexpected external reference".into());
            }
            scene_cpu_close = self.scene_resources.cpu_close_report(scene);
            self.scene = None;
            self.scene_world_progress = None;
        }
        self.close_driving_scene_caches()?;
        self.services
            .audio
            .clear_listener()
            .map_err(|error| format!("Driving PCM listener cleanup: {error:?}"))?;
        self.scene_cache_bucket = None;
        self.scene_driver
            .release_empty_registry_storage()
            .map_err(|error| format!("Driver scene registry storage: {error:?}"))?;
        if model_to_check.is_some_and(|model| self.arena.frame(model.root()).is_ok()) {
            return Err("Car inspection retained an unexpected model reference".into());
        }
        let cpu = self.arena.loaded_visual_resources().map(|r| r.backend());
        let cleanup = CarRuntimeCleanup {
            actor_count,
            actor_registry_entries: entries,
            heap_allocations: self.memory.allocations().count(),
            graphics: cpu.map_or(0, |r| r.live_graphics()) + self.scene_storage.live_graphics(),
            textures: cpu.map_or(0, |r| r.live_textures())
                + self.scene_resources.snow_texture().is_some() as usize,
            cpu_allocations: cpu.map_or(0, |r| r.live_allocations())
                + self.scene_storage.live_pixels()
                + self.scene_storage.live_strings(),
            audio_sounds: self
                .services
                .audio
                .live_sound_count()
                .map_err(|e| e.to_string())?,
            audio_voices: self
                .services
                .audio
                .live_voice_count()
                .map_err(|e| e.to_string())?,
            audio_sources: self
                .services
                .audio
                .live_source_count()
                .map_err(|e| e.to_string())?,
            audio_listener_pose: self
                .services
                .audio
                .listener_present()
                .map_err(|e| e.to_string())?,
            driver_sound_frames: self
                .scene_driver
                .host_sound_backend()
                .map_or(Ok(0), |backend| backend.sounds().map(|sounds| sounds.len()))
                .map_err(|e| format!("{e:?}"))?,
            driver_listeners: self
                .scene_driver
                .host_sound_backend()
                .map_or(Ok(0), |backend| {
                    backend.listeners().map(|listeners| listeners.len())
                })
                .map_err(|e| format!("{e:?}"))?,
            scene_cpu_close,
            driver_scene_registrations: self.scene_driver.scenes().len(),
            driver_scene_capacity: self.scene_driver.capacity(),
            retained_scene_prefix_frames: self
                .scene
                .and_then(|scene| self.arena.scene(scene).ok())
                .map_or(0, |scene| 1 + scene.sectors.len()),
        };
        if cleanup.driver_scene_registrations != 0
            || cleanup.driver_scene_capacity != 0
            || cleanup.heap_allocations != 0
            || cleanup.graphics != 0
            || cleanup.textures != 0
            || cleanup.cpu_allocations != 0
            || cleanup.audio_sounds != 0
            || cleanup.audio_voices != 0
            || cleanup.audio_sources != 0
            || cleanup.audio_listener_pose
            || cleanup.driver_sound_frames != 0
            || cleanup.driver_listeners != 0
        {
            return Err(format!(
                "Car inspection retained CPU resources: {cleanup:?}; remaining heap: {:?}",
                self.memory.allocations().collect::<Vec<_>>()
            ));
        }
        self.driving = None;
        self.startup_patches = None;
        Ok(cleanup)
    }
}
struct InspectionScreen {
    size: [i32; 2],
    viewport: [i32; 4],
}
impl crate::scene_resources::SceneScreen for InspectionScreen {
    type Error = std::convert::Infallible;
    fn screen_sy(&mut self) -> Result<i32, Self::Error> {
        Ok(self.size[1])
    }
    fn screen_sx(&mut self) -> Result<i32, Self::Error> {
        Ok(self.size[0])
    }
}
impl crate::driver_scene::DriverSceneScreen for InspectionScreen {
    fn viewport_rectangle(&mut self) -> Result<[i32; 4], Self::Error> {
        Ok(self.viewport)
    }
}
struct InstallationImages<'a>(&'a lhr_assets::Installation);
impl crate::itexture::TextureImageSource for InstallationImages<'_> {
    fn read_image(&mut self, path: &[u8]) -> Result<Vec<u8>, crate::itexture::TextureImageError> {
        use crate::itexture::TextureImageError;
        let path = std::str::from_utf8(path).map_err(|_| TextureImageError::Unsupported)?;
        self.0.read_original_path(path).map_err(|error| {
            if error.0 == "original asset unavailable" {
                TextureImageError::Missing
            } else {
                TextureImageError::Backend(error.to_string())
            }
        })
    }
}
fn original_model_input(path: &str) -> Result<String, String> {
    if !path.is_ascii() || path.contains('\0') {
        return Err("Car inspection requires an ASCII relative model path".into());
    }
    let input = path.replace('/', "\\");
    let dot = input
        .rfind('.')
        .ok_or("Car inspection model path has no extension")?;
    Ok(format!("{}.i3d", &input[..dot]))
}

#[cfg(test)]
#[path = "car_runtime_lifecycle_tests.rs"]
mod lifecycle_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_asset_retains_then_explicitly_closes_the_inspection_context() {
        struct Directory(std::path::PathBuf);
        impl Drop for Directory {
            fn drop(&mut self) {
                std::fs::remove_dir_all(&self.0).unwrap();
            }
        }
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("lhr-car-open-{}-{unique}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        let directory = Directory(path);
        let installation = lhr_assets::Installation::open(&directory.0).unwrap();
        let mut failure =
            match CarRuntime::open(installation, "models/missing.4ds", Default::default()) {
                Ok(_) => panic!("missing asset accepted"),
                Err(failure) => failure,
            };
        assert!(!failure.partial.summary().constructed);
        let cleanup = failure.partial.shutdown().unwrap();
        assert_eq!(
            (
                cleanup.actor_count,
                cleanup.actor_registry_entries,
                cleanup.heap_allocations
            ),
            (0, 0, 0)
        );
        assert_eq!(
            (cleanup.graphics, cleanup.textures, cleanup.cpu_allocations),
            (0, 0, 0)
        );
        assert_eq!(cleanup.retained_scene_prefix_frames, 0);
        assert_eq!(
            (
                cleanup.driver_scene_registrations,
                cleanup.driver_scene_capacity
            ),
            (0, 0)
        );
        assert!(failure.partial.shutdown().is_err());
    }

    #[test]
    fn scene_close_releases_live_sound_from_the_same_driving_pcm_owner() {
        struct Directory(std::path::PathBuf);
        impl Drop for Directory {
            fn drop(&mut self) {
                std::fs::remove_dir_all(&self.0).unwrap();
            }
        }
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "lhr-car-scene-sound-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        let directory = Directory(path);
        let installation = lhr_assets::Installation::open(&directory.0).unwrap();
        let mut failure =
            match CarRuntime::open(installation, "models/missing.4ds", Default::default()) {
                Ok(_) => panic!("missing car asset accepted"),
                Err(failure) => failure,
            };
        let runtime = &mut failure.partial;
        let audio = runtime.driving_audio();
        assert!(audio.same_owner(&runtime.services.audio));
        runtime
            .arena
            .bind_host_sound_backend(crate::frame_sound::HostSoundBackend::new(1, &audio))
            .unwrap();
        let scene = runtime.scene.unwrap();
        let primary = runtime.arena.scene(scene).unwrap().sectors[0];
        let sound = runtime.arena.create_owned_sound_frame().unwrap();
        runtime.arena.scene_add_frame(scene, sound.frame()).unwrap();
        runtime.arena.release(sound.frame()).unwrap();
        runtime.arena.add_sound_to_sector(primary, sound).unwrap();
        assert_eq!(audio.live_sound_count().unwrap(), 1);
        assert_eq!(
            runtime.arena.scene(scene).unwrap().frames(),
            &[sound.frame()]
        );
        let cleanup = runtime.shutdown().unwrap();
        assert!(cleanup.scene_cpu_close.is_some());
        assert_eq!(cleanup.audio_sounds, 0);
        assert_eq!(cleanup.audio_voices, 0);
        assert_eq!(cleanup.audio_sources, 0);
        assert!(!cleanup.audio_listener_pose);
        assert_eq!(cleanup.driver_sound_frames, 0);
        assert_eq!(cleanup.driver_listeners, 0);
        assert!(!audio.owns(sound.frame().0));
        assert!(runtime.shutdown().is_err());
    }
}
