//! Public synthetic original-source intake; no native gameplay equivalence claim.
use super::*;
use crate::canonical_scene2_models::CanonicalScene2Models;
use crate::driver_scene::{
    DriverSceneContext, DriverSceneOwner, DriverSceneScreen, NativeSceneSoundDriver,
};
use crate::frame_transform::FrameKey;
use crate::itexture::{CpuTextureStorage, TextureImageError, TextureImageSource, TextureResources};
use crate::mission_resources::{CpuMissionCacheMemory, MissionCacheKind, MissionCaches};
use crate::mission_scene_loading::MissionSceneLoader;
use crate::model_loading::{ModelArena, ModelKey};
use crate::scene_ownership::SceneKey;
use crate::scene_resources::{SceneResources, SceneScreen};
use crate::vehicle_frame_backend::VehicleFrameMemory;
use crate::vehicle_init::{
    VehicleComponentTailMemory, VehicleComponentTailScratch, VEHICLE_INIT_STACK_BYTES,
};
use lhr_assets::Installation;
struct Screen;
impl SceneScreen for Screen {
    type Error = ();
    fn screen_sy(&mut self) -> Result<i32, ()> {
        Ok(600)
    }
    fn screen_sx(&mut self) -> Result<i32, ()> {
        Ok(800)
    }
}
impl DriverSceneScreen for Screen {
    fn viewport_rectangle(&mut self) -> Result<[i32; 4], ()> {
        Ok([0, 0, 800, 600])
    }
}
struct Images;
impl TextureImageSource for Images {
    fn read_image(&mut self, _: &[u8]) -> Result<Vec<u8>, TextureImageError> {
        Err(TextureImageError::Missing)
    }
}
fn words(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}
fn model() -> Vec<u8> {
    let mut b = b"4DS\0".to_vec();
    b.extend(29u16.to_le_bytes());
    b.extend([0; 8]);
    b.extend(1u16.to_le_bytes()); // one untextured material
    b.extend(0x10000000u32.to_le_bytes()); // genuine two-sided source material
    b.extend(words(&[0., 0., 0., 1., 1., 1., 0., 0., 0., 1.]));
    b.push(0);
    b.extend(1u16.to_le_bytes());
    b.extend([1, 0, 0, 0x2a]);
    b.extend(0u16.to_le_bytes());
    b.extend(words(&[0., 0., 0., 1., 1., 1., 1., 0., 0., 0.]));
    b.extend([0x11, 4]);
    b.extend(b"part");
    let properties = b";.X100M100;";
    b.push(properties.len() as u8);
    b.extend(properties);
    b.extend(0u16.to_le_bytes());
    b.push(1);
    b.extend(0f32.to_le_bytes());
    b.extend(3u16.to_le_bytes());
    for p in [[0., 0., 0.], [2., 0., 0.], [0., 2., 0.]] {
        b.extend(words(&[p[0], p[1], p[2], 0., 0., 1., 0., 0.]));
    }
    b.push(1);
    b.extend(1u16.to_le_bytes());
    for index in [0u16, 1, 2, 1] {
        b.extend(index.to_le_bytes());
    }
    b.push(0);
    b
}
fn scene2() -> Vec<u8> {
    let c = crate::original_data_chunk::chunk;
    let object = c(
        0x4010,
        &[
            c(0x4011, &9u32.to_le_bytes()),
            c(0x10, b"f\0"),
            c(0x2012, b"fixture.i3d\0"),
            c(0x20, &words(&[0.; 3])),
            c(0x2c, &words(&[0.; 3])),
            c(0x22, &words(&[1., 0., 0., 0.])),
            c(0x2d, &words(&[1.; 3])),
            c(0x4020, &c(0x10, b"Primary sector\0")),
        ]
        .concat(),
    );
    c(0x4c53, &[c(1, &[5, 0, 1, 0]), c(0x4000, &object)].concat())
}
fn klz() -> Vec<u8> {
    fn put(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    let mut b = vec![0; 36 + 104];
    b[..4].copy_from_slice(b"GifC");
    for (offset, value) in [(4, 5), (8, 36), (12, 1), (24, 28), (28, 0)] {
        put(&mut b, offset, value);
    }
    b[32] = b'f';
    for (index, value) in [0f32, 0., 10., 10., 10., 10.].iter().enumerate() {
        put(&mut b, 36 + index * 4, value.to_bits());
    }
    for offset in [24, 28, 0x40] {
        put(&mut b, 36 + offset, 1);
    }
    put(&mut b, 36 + 0x20, 10f32.to_bits());
    b.extend(words(&[0., 10., 0., 10.]));
    b.extend([0; 4]);
    let shape = b.len();
    b.resize(shape + 32, 0);
    put(&mut b, shape, 0x2900_0081);
    b[shape + 8..shape + 32].copy_from_slice(&words(&[0., 0., 0., 2., 2., 2.]));
    b.extend([0; 4]);
    let cell = b.len();
    b.resize(cell + 24, 0);
    put(&mut b, cell, 1);
    put(&mut b, cell + 12, 10f32.to_bits());
    put(&mut b, cell + 16, 0x8100_0000);
    b
}
struct Fixture {
    directory: std::path::PathBuf,
    installation: Installation,
    arena: ModelArena,
    scene: SceneKey,
    driver: DriverSceneOwner,
    resources: SceneResources,
    textures: TextureResources,
    storage: CpuTextureStorage,
    world: DrivingWorldAssets,
    correspondence: CanonicalScene2Models,
    loader: MissionSceneLoader,
    caches: MissionCaches,
    cache_memory: CpuMissionCacheMemory,
    memory: VehicleFrameMemory,
    state: SimulationState,
    services: super::super::CarRuntimeServices,
    root: FrameKey,
    source: FrameKey,
}
impl Fixture {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "lhr-scene2-crash-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir_all(directory.join("missions/synthetic")).unwrap();
        std::fs::create_dir(directory.join("models")).unwrap();
        std::fs::write(directory.join("models/fixture.4ds"), model()).unwrap();
        let mut empty = b"4DS\0".to_vec();
        empty.extend(29u16.to_le_bytes());
        empty.extend([0; 13]);
        std::fs::write(directory.join("missions/synthetic/scene.4ds"), empty).unwrap();
        let bytes = scene2();
        std::fs::write(directory.join("missions/synthetic/scene2.bin"), &bytes).unwrap();
        std::fs::write(directory.join("missions/synthetic/tree.klz"), klz()).unwrap();
        let installation = Installation::open(&directory).unwrap();
        let world = DrivingWorldAssets::load(&installation, "synthetic").unwrap();
        assert_eq!(world.summary().resolved_links, 1);
        let mut arena = ModelArena::default();
        let mut resources = SceneResources::default();
        let mut textures = TextureResources::new().unwrap();
        let mut storage = CpuTextureStorage::default();
        let mut driver = DriverSceneOwner::default();
        let scene = driver
            .create_scene(
                &mut DriverSceneContext {
                    arena: &mut arena,
                    resources: &mut resources,
                    textures: &mut textures,
                    storage: &mut storage,
                },
                &mut Screen,
                &mut Images,
                NativeSceneSoundDriver::Absent,
            )
            .unwrap();
        let mut caches = MissionCaches::empty().unwrap();
        let mut cache_memory = CpuMissionCacheMemory::default();
        caches
            .add_dir(MissionCacheKind::Model, b"Models", &mut cache_memory)
            .unwrap();
        let mut loader = MissionSceneLoader::new(&arena, scene)
            .map_err(|e| e.detail)
            .unwrap();
        loader
            .load_prefix_with_model_cache(
                &mut driver,
                &mut arena,
                &resources,
                &mut Screen,
                crate::game_camera_projection::GameCameraProjection {
                    global_mode: 0,
                    screen_flag_46f4: false,
                },
                &bytes,
                &mut caches,
                &installation,
                &mut cache_memory,
            )
            .unwrap();
        assert_eq!(loader.loaded_objects.len(), 1);
        let root = FrameKey(loader.loaded_objects[0].frame.unwrap().identity());
        let source = FrameKey(*world.scene2_models().keys().next().unwrap());
        let correspondence =
            CanonicalScene2Models::bind(&arena, scene, &world, &bytes, &loader.loaded_objects)
                .unwrap();
        let mut memory = VehicleFrameMemory::default();
        memory.map_bytes(0x637000, &vec![0; 0x12000]).unwrap();
        memory.map_bytes(0x8000, &[0; 0x100]).unwrap();
        memory.map_bytes(0x9000, &vec![0; 0x4000]).unwrap();
        memory.write_word(0x63788c, 0x8000).unwrap();
        memory.write_word(0x8024, 0x9000).unwrap();
        Self {
            directory,
            installation,
            arena,
            scene,
            driver,
            resources,
            textures,
            storage,
            world,
            correspondence,
            loader,
            caches,
            cache_memory,
            memory,
            state: Default::default(),
            services: Default::default(),
            root,
            source,
        }
    }
    fn draws(&mut self) -> usize {
        self.arena.publish_runtime_buffers().unwrap();
        crate::car_inspection::snapshot_arena_tree(&mut self.arena, self.root)
            .unwrap()
            .scene
            .meshes
            .len()
    }
    fn hit(&self) -> bool {
        self.world
            .collision()
            .test_line_v(
                &CollisionLineRequest {
                    origin: [1., 3., 1.],
                    delta: [0., -4., 0.],
                    selector: -1,
                    reject_mask: 0,
                },
                &mut CollisionLineOutputs::default(),
                &mut Default::default(),
            )
            .unwrap()
            .is_some()
    }
    fn backend(
        &mut self,
        f: impl FnOnce(&mut CarRuntimeSimulationBackend<'_, '_, VehicleFrameMemory>),
    ) {
        let mut vehicle = [0; 0x2000];
        let mut scratch = VehicleComponentTailScratch {
            address: 0x700000,
            bytes: [0; VEHICLE_INIT_STACK_BYTES],
        };
        let audio = self.services.audio.clone();
        let mut frames = VehicleFrameBackend::with_installation(
            &mut self.arena,
            &mut self.memory,
            &self.installation,
        )
        .with_game_host(&mut self.services);
        let scene_address = frames.frame_address(self.scene.root()).unwrap();
        frames.host_mut().write_word(0x8010, scene_address).unwrap();
        let memory =
            VehicleComponentTailMemory::synthetic_caller(0x100000, &mut vehicle, &mut scratch);
        let mut backend = CarRuntimeSimulationBackend::new(
            &mut frames,
            memory,
            &mut self.world,
            audio,
            &mut self.state,
        )
        .with_scene2_models(Some(&self.correspondence));
        f(&mut backend);
    }
    fn finish(mut self) {
        self.resources
            .close_constructed_scene(
                &mut self.arena,
                self.scene,
                &mut self.textures,
                &mut self.storage,
            )
            .unwrap();
        self.caches
            .destruct(
                MissionCacheKind::Model,
                &mut self.arena,
                &mut self.cache_memory,
            )
            .unwrap();
        self.driver
            .release_scene(
                &mut DriverSceneContext {
                    arena: &mut self.arena,
                    resources: &mut self.resources,
                    textures: &mut self.textures,
                    storage: &mut self.storage,
                },
                self.scene,
            )
            .unwrap();
        self.driver.release_empty_registry_storage().unwrap();
        self.arena.reclaim_unused_visual_buffers().unwrap();
        let hidden = self.memory.read_word(0x9014).unwrap();
        self.memory.free(hidden).unwrap();
        // The synthetic process caller also owns the emptied base actor
        // registry capacity; actor destruction removes entries, not capacity.
        let registry = self.memory.read_word(0x63d2bc).unwrap();
        assert_eq!(self.memory.read_word(0x63d2c0).unwrap(), registry);
        assert_eq!(self.memory.read_word(0x63d2c8).unwrap(), 0);
        self.memory.free(registry).unwrap();
        assert_eq!(self.memory.allocations().count(), 0);
        assert_eq!(self.cache_memory.live_allocations(), 0);
        assert_eq!(self.arena.live_frame_count(), 0);
        assert_eq!(self.services.audio.live_sound_count().unwrap(), 0);
        assert!(self
            .correspondence
            .resolve(&self.arena, &self.world, self.source)
            .is_err());
        std::fs::remove_dir_all(&self.directory).unwrap();
    }
}
#[test]
fn completed_false_actor_worker_hides_actual_scene2_draw_and_disables_klz_contact() {
    crate::native_precision::Pc24PrecisionState::default().run(|| {
        let mut fixture = Fixture::new();
        assert_eq!(fixture.draws(), 1);
        assert!(fixture.hit());
        let info = fixture.world.collision().primitives()[0];
        assert_eq!(info.material(), 0x29);
        assert_ne!(info.properties & 0xc0, 0);
        fixture.backend(|backend| {
            // Empty template search paths are the genuine false actor-worker
            // result, intentionally ignored by CreateCrashModel.
            crate::vehicle_car_contacts::VehicleCarContactBackend::car_contact_crash_model(
                backend,
                crate::vehicle_car_contacts::VehicleCarCrashCall {
                    descriptor: info.identity,
                    point: [1., 1., 1.],
                    velocity: [1., 0., 0.],
                    scale: 1.,
                },
            )
            .unwrap();
            assert!(backend.state.crash_pieces.is_empty());
            assert!(backend.state.crash_pending.is_empty());
        });
        assert_eq!(fixture.draws(), 0);
        assert!(!fixture.hit());
        fixture.finish();
    });
}
#[test]
fn missing_completed_correspondence_stops_at_set_on_after_collision_and_registry_prefix() {
    crate::native_precision::Pc24PrecisionState::default().run(|| {
        let mut f = Fixture::new();
        f.correspondence =
            CanonicalScene2Models::bind(&f.arena, f.scene, &f.world, &scene2(), &[]).unwrap();
        let frame = f.world.crash_frame_identity(f.source).unwrap();
        f.backend(|b| {
            let error = crate::vehicle_car_contacts::vehicle_create_crash_model(
                b,
                frame,
                [0.; 3],
                [1., 0., 0.],
                1.,
            )
            .unwrap_err();
            assert!(format!("{error:?}").contains("no completed model"));
            assert_ne!(b.read_word(0x9018).unwrap(), 0); // hidden registry prefix reached
        });
        assert_eq!(f.draws(), 1);
        assert!(!f.hit());
        assert_ne!(f.world.frame_graph().state(f.source).unwrap().flags & 1, 0);
        f.finish();
    });
}
#[test]
fn correspondence_checks_source_owner_reload_disposal_and_live_child_replacement() {
    crate::native_precision::Pc24PrecisionState::default().run(|| {
        let mut f = Fixture::new();
        assert!(CanonicalScene2Models::bind(
            &f.arena,
            f.scene,
            &f.world,
            b"different",
            &f.loader.loaded_objects
        )
        .is_err());
        assert!(f
            .correspondence
            .resolve(&ModelArena::default(), &f.world, f.source)
            .is_err());
        let other = DrivingWorldAssets::load(&f.installation, "synthetic").unwrap();
        assert!(f
            .correspondence
            .resolve(&f.arena, &other, f.source)
            .is_err());
        let old = f.arena.model(ModelKey(f.root)).unwrap().frames[0];
        let new = f
            .arena
            .create_owned_object_visual(crate::visual_replacement::LIT_OBJECT_VISUAL)
            .unwrap_or_else(|e| panic!("{:?}", e.error))
            .unwrap();
        f.arena.duplicate_owned_object_visual(new, old).unwrap();
        f.arena.link_frame(new, Some(f.root), 0).unwrap();
        f.arena.link_frame(old, None, 0).unwrap();
        f.arena.delete_frame(ModelKey(f.root), old).unwrap();
        f.arena.add_frame(ModelKey(f.root), new).unwrap();
        f.arena.release(new).unwrap();
        assert_eq!(
            f.correspondence
                .resolve(&f.arena, &f.world, f.source)
                .unwrap(),
            Some(f.root)
        );
        f.arena.close(ModelKey(f.root)).unwrap();
        assert!(f
            .correspondence
            .resolve(&f.arena, &f.world, f.source)
            .is_err());
        f.arena.scene_delete_frame(f.scene, f.root).unwrap();
        assert!(f
            .correspondence
            .resolve(&f.arena, &f.world, f.source)
            .is_err());
        f.finish();
    });
}

#[test]
fn actor_worker_error_retains_collision_prefix_without_hiding_original() {
    crate::native_precision::Pc24PrecisionState::default().run(|| {
        let mut f = Fixture::new();
        let frame = f.world.crash_frame_identity(f.source).unwrap();
        // Invalid original cache vector extent injects a reached worker error,
        // after a real temporary model factory and the collision disable.
        f.memory.write_word(0x647dd8, 1).unwrap();
        f.backend(|b| {
            let error = crate::vehicle_car_contacts::vehicle_create_crash_model(
                b,
                frame,
                [0.; 3],
                [1., 0., 0.],
                1.,
            )
            .unwrap_err();
            assert!(format!("{error:?}").contains("StorageRange"));
            assert_eq!(b.read_word(0x9018).unwrap(), 0); // final registry not reached
            assert_eq!(b.state.crash_pending.len(), 1);
            crate::crash_actors::cleanup_crash_pending(b).unwrap();
            assert!(b.state.crash_pending.is_empty());
        });
        assert_eq!(f.draws(), 1);
        assert!(!f.hit());
        assert_ne!(f.world.frame_graph().state(f.source).unwrap().flags & 1, 0);
        f.finish();
    });
}

#[test]
fn completed_crash_keeps_real_physical_pieces_owned_after_original_draw_is_hidden() {
    crate::native_precision::Pc24PrecisionState::default().run(|| {
        let mut f = Fixture::new();
        let frame = f.world.crash_frame_identity(f.source).unwrap();
        let grid = f.world.dynamic_grid_source();
        let original = f.root;
        f.backend(|b| {
            crate::dynamic_collision_manager::initialize_dynamic_grid(b, 0x647f48, grid).unwrap();
            // The real template loader reads the synthetic installed 4DS.
            let path = b"models\\";
            let name = b.allocate(path.len() as u32 + 9).unwrap();
            b.write_word(name, 1).unwrap();
            b.write_word(name + 4, path.len() as u32).unwrap();
            for (i, byte) in path.iter().copied().chain([0]).enumerate() {
                b.write_byte(name + 8 + i as u32, byte).unwrap();
            }
            let table = b.allocate(4).unwrap();
            b.write_word(table, name).unwrap();
            for (address, value) in [
                (0x647de8, table),
                (0x647dec, table + 4),
                (0x647df0, table + 4),
                (0x647df4, 8),
            ] {
                b.write_word(address, value).unwrap();
            }
            assert!(crate::vehicle_car_contacts::vehicle_create_crash_model(
                b,
                frame,
                [0.5, 0.5, 1.],
                [0., 0., -2.],
                100.
            )
            .unwrap());
            assert_eq!(b.state.crash_pieces.len(), 1);
            assert_eq!(b.frames.arena().state(original).unwrap().flags & 1, 0);
            assert!(b.state.crash_pending.is_empty());
            let piece = b.state.crash_pieces[0].frame;
            let key = b.frames.frame_key(piece).unwrap();
            assert_ne!(b.frames.arena().state(key).unwrap().flags & 1, 0);
            assert_eq!(b.read_word(0x63d6d4).unwrap(), 1);
            assert_eq!(
                crate::car_inspection::snapshot_arena_tree(b.frames.arena_mut(), key)
                    .unwrap()
                    .scene
                    .meshes
                    .len(),
                1
            );
            // Genuine actor GameDone/destructor, template release, and dynamic
            // manager destruction retain/release their original owners.
            crate::crash_actor::crash_clear_temporary_actors(b, 0x9000).unwrap();
            assert!(b.frames.arena().frame(key).is_err());
            assert_eq!(b.read_word(0x63d6d4).unwrap(), 0);
            crate::crash_actors::cleanup_crash_model_cache(b).unwrap();
            let scratch = b.allocate(16).unwrap();
            crate::dynamic_collision_manager::release_owned_dynamic_grid(b, 0x647f48, scratch)
                .unwrap();
            b.free(scratch).unwrap();
            b.free(name).unwrap();
            b.free(table).unwrap();
            for offset in [0xec, 0xfc, 0x120, 0x130] {
                let begin = b.read_word(0x9000 + offset + 4).unwrap();
                b.free(begin).unwrap();
            }
        });
        assert_eq!(f.draws(), 0);
        assert!(!f.hit());
        f.finish();
    });
}

#[test]
#[ignore = "requires a Vulkan or Metal adapter; run with --ignored"]
fn actual_scene2_crash_changes_gpu_snapshot_without_a_visibility_refresh() {
    crate::native_precision::Pc24PrecisionState::default().run(|| {
        let mut f = Fixture::new();
        f.arena.publish_runtime_buffers().unwrap();
        let before = crate::car_inspection::snapshot_arena_tree(&mut f.arena, f.root).unwrap();
        assert_eq!(before.scene.meshes.len(), 1);
        let camera = lhr_render::Camera::fit(&before.scene).unwrap();
        let instance = lhr_render::instance();
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let mut renderer = pollster::block_on(lhr_render::Renderer::with_adapter(
            &adapter,
            &before.scene,
            wgpu::TextureFormat::Rgba8Unorm,
            false,
        ))
        .unwrap();
        let before_pixels = renderer.readback(64, 64, &camera).unwrap();
        if let Ok(directory) = std::env::var("LHR_WORLD_CAPTURE_DIR") {
            renderer
                .capture(
                    64,
                    64,
                    &camera,
                    &std::path::Path::new(&directory).join("scene2-before.png"),
                )
                .unwrap();
        }
        let frame = f.world.crash_frame_identity(f.source).unwrap();
        f.backend(|b| {
            assert!(crate::vehicle_car_contacts::vehicle_create_crash_model(
                b,
                frame,
                [1., 1., 1.],
                [1., 0., 0.],
                1.,
            )
            .unwrap());
        });
        let after = crate::car_inspection::snapshot_arena_tree(&mut f.arena, f.root).unwrap();
        assert!(after.scene.meshes.is_empty());
        renderer.replace_meshes(0..1, &after.scene.meshes).unwrap();
        let after_pixels = renderer.readback(64, 64, &camera).unwrap();
        assert_ne!(
            before_pixels, after_pixels,
            "the original draw must have reached the GPU"
        );
        assert!(after_pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p == &after_pixels[..4]));
        if let Ok(directory) = std::env::var("LHR_WORLD_CAPTURE_DIR") {
            renderer
                .capture(
                    64,
                    64,
                    &camera,
                    &std::path::Path::new(&directory).join("scene2-after.png"),
                )
                .unwrap();
        }
        assert!(!f.hit());
        f.finish();
    });
}
