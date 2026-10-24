//! Standalone CPU car inspection owner. It composes reconstructed construction,
//! virtual Init and destruction with an explicit initial Game/driver context.
//! C_car::GameInit, mission population, simulation and driving are not invoked.
use crate::{
    car_effects::CarTrackResources,
    loaded_visual_resources::MaterialLoadContext,
    model_loading::{ModelArena, ModelKey},
    scene_ownership::SceneKey,
    vehicle_bodycol::{VehicleBodycolScratch, VehicleBodycolStorage},
    vehicle_frame_backend::{VehicleFrameBackend, VehicleFrameMemory, VehicleMemoryError},
    vehicle_init::*,
};
#[path = "car_runtime_catalog.rs"]
mod catalog;
pub use catalog::CarInitAnimationCatalog;

const CAR: u32 = 0x10000;
const INIT_STACK: u32 = 0x800000;
const VEHICLE_STACK: u32 = INIT_STACK - 0x1d0;
const CONSTRUCTOR_STACK: u32 = 0x820000;

#[derive(Clone, Copy, Debug)]
pub struct CarRuntimeOptions {
    /// Explicit host allocation/caller-storage seed, not a native heap image.
    pub allocator_fill: u8,
    pub random_seed: u32,
    pub material_context: MaterialLoadContext,
}
impl Default for CarRuntimeOptions {
    fn default() -> Self {
        Self {
            allocator_fill: 0,
            random_seed: 1,
            material_context: MaterialLoadContext::ORIGINAL_DRIVER_INIT_DEFAULTS,
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CarRuntimeCleanup {
    pub actor_count: u32,
    pub actor_registry_entries: usize,
    pub heap_allocations: usize,
    pub graphics: usize,
    pub textures: usize,
    pub cpu_allocations: usize,
    /// Host inspection uses the bounded Scene constructor prefix only; these
    /// frames are not reported as a completed native Scene destruction.
    pub retained_scene_prefix_frames: usize,
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
    installation: lhr_assets::Installation,
    arena: ModelArena,
    memory: VehicleFrameMemory,
    tracks: CarTrackResources,
    model: Option<ModelKey>,
    scene: Option<SceneKey>,
    construction: Option<CarConstructionState>,
    car: Option<VehicleInitState>,
    summary: CarRuntimeSummary,
    car_destructor_started: bool,
    car_destructor_complete: bool,
    shutdown_started: bool,
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
        let tracks = CarInitAnimationCatalog::resources().expect("bounded unique semantic catalog");
        let mut runtime = Self {
            installation,
            arena: ModelArena::default(),
            memory: VehicleFrameMemory::default(),
            tracks,
            model: None,
            scene: None,
            construction: None,
            car: None,
            summary: CarRuntimeSummary {
                constructed: false,
                initialized: false,
                seats: 0,
                cached_tracks: 0,
                source_frames: 0,
            },
            car_destructor_started: false,
            car_destructor_complete: false,
            shutdown_started: false,
        };
        if let Err(message) = runtime.initialize(path, options) {
            return Err(CarRuntimeOpenFailure {
                message,
                partial: Box::new(runtime),
            });
        }
        Ok(runtime)
    }
    fn initialize(&mut self, path: &str, options: CarRuntimeOptions) -> Result<(), String> {
        self.prepare_game_context(options)?;
        self.arena
            .visual_resource_owner_mut()
            .map_err(|e| e.to_string())?
            .set_material_load_context(options.material_context);
        self.arena
            .visual_resource_owner_mut()
            .map_err(|e| e.to_string())?
            .enable_inspection_rgba8_color_key()
            .map_err(|e| format!("Car inspection format: {e:?}"))?;
        let scene = self
            .arena
            .construct_scene_owner_prefix()
            .map_err(|e| e.to_string())?;
        self.scene = Some(scene);
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
    fn prepare_game_context(&mut self, options: CarRuntimeOptions) -> Result<(), String> {
        let fill = options.allocator_fill;
        let regions: Vec<(u32, Vec<u8>)> = vec![
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
            (0x63d2bc, vec![0; 16]),
            (0x63a200, options.random_seed.to_le_bytes().to_vec()),
        ];
        for (address, bytes) in regions {
            self.memory
                .map_bytes(address, &bytes)
                .map_err(|e| format!("Game inspection context: {e:?}"))?;
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
    pub fn model(&self) -> Option<ModelKey> {
        self.model
    }
    pub fn cached_track_count(&self) -> usize {
        self.tracks.cached_tracks().count()
    }
    /// Runs only C_car destruction. Shared Game track metadata and actor-vector
    /// capacity remain owned until the later explicit Game-context shutdown.
    pub fn destruct_car(&mut self) -> Result<(), String> {
        if self.car_destructor_started {
            return Err("Car destruction already started".into());
        }
        self.car_destructor_started = true;
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
        self.car_destructor_complete = true;
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
        if !self.car_destructor_complete {
            self.destruct_car()?;
        }
        if let Some(model) = self.model {
            self.arena
                .link_frame(model.root(), None, 0)
                .map_err(|e| e.to_string())?;
            if let Some(scene) = self.scene {
                if self
                    .arena
                    .frame(model.root())
                    .map_err(|e| e.to_string())?
                    .owner
                    == Some(scene.into())
                {
                    self.arena
                        .scene_delete_frame(scene, model.root())
                        .map_err(|e| e.to_string())?;
                }
            }
            self.arena.close(model).map_err(|e| e.to_string())?;
            self.arena
                .release(model.root())
                .map_err(|e| e.to_string())?;
            if self.arena.frame(model.root()).is_ok() {
                return Err("Car inspection retained an unexpected model reference".into());
            }
        }
        self.arena
            .reclaim_unused_visual_buffers()
            .map_err(|e| e.to_string())?;
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
        self.tracks.reset_track_cache();
        let cpu = self.arena.loaded_visual_resources().map(|r| r.backend());
        let cleanup = CarRuntimeCleanup {
            actor_count,
            actor_registry_entries: entries,
            heap_allocations: self.memory.allocations().count(),
            graphics: cpu.map_or(0, |r| r.live_graphics()),
            textures: cpu.map_or(0, |r| r.live_textures()),
            cpu_allocations: cpu.map_or(0, |r| r.live_allocations()),
            retained_scene_prefix_frames: self
                .scene
                .and_then(|scene| self.arena.scene(scene).ok())
                .map_or(0, |scene| 1 + scene.sectors.len()),
        };
        if cleanup.heap_allocations != 0
            || cleanup.graphics != 0
            || cleanup.textures != 0
            || cleanup.cpu_allocations != 0
        {
            return Err(format!(
                "Car inspection retained CPU resources: {cleanup:?}"
            ));
        }
        Ok(cleanup)
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
        assert_eq!(cleanup.retained_scene_prefix_frames, 5);
        assert!(failure.partial.shutdown().is_err());
    }
}
