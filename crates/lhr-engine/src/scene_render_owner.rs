//! Real driver Scene ownership for the application scene inspection caller.
use super::*;
use crate::driver_scene::{
    DriverSceneContext, DriverSceneOwner, DriverSceneScreen, NativeSceneSoundDriver,
};
use crate::itexture::{CpuTextureStorage, TextureImageError, TextureImageSource, TextureResources};
use crate::mission_resources::{CpuMissionCacheMemory, MissionCacheKind, MissionCaches};
use crate::mission_scene_loading::{MissionSceneLoadError, MissionSceneLoader};
use crate::scene_resources::{SceneResources, SceneScreen};
use std::convert::Infallible;
struct Screen([u32; 2]);
impl SceneScreen for Screen {
    type Error = Infallible;
    fn screen_sx(&mut self) -> Result<i32, Self::Error> {
        Ok(self.0[0] as i32)
    }
    fn screen_sy(&mut self) -> Result<i32, Self::Error> {
        Ok(self.0[1] as i32)
    }
}
impl DriverSceneScreen for Screen {
    fn viewport_rectangle(&mut self) -> Result<[i32; 4], Self::Error> {
        Ok([0, 0, self.0[0] as i32, self.0[1] as i32])
    }
}
struct Images<'a>(&'a lhr_assets::Installation);
impl TextureImageSource for Images<'_> {
    fn read_image(&mut self, path: &[u8]) -> Result<Vec<u8>, TextureImageError> {
        let path = std::str::from_utf8(path).map_err(|_| TextureImageError::Unsupported)?;
        self.0.read_original_path(path).map_err(|e| {
            if e.0 == "original asset unavailable" {
                TextureImageError::Missing
            } else {
                TextureImageError::Backend(e.to_string())
            }
        })
    }
}
pub struct CanonicalSceneState {
    pub arena: ModelArena,
    pub scene: Option<SceneKey>,
    resources: SceneResources,
    driver: DriverSceneOwner,
    textures: TextureResources,
    storage: CpuTextureStorage,
    world: Option<crate::model_loading::CpuSceneWorldProgress>,
    pub loader: Option<MissionSceneLoader>,
    pub frontier: Option<MissionSceneLoadError>,
    caches: MissionCaches,
    cache_memory: CpuMissionCacheMemory,
    precision: crate::native_precision::Pc24PrecisionState,
    closed: bool,
    pub lighting: Vec<Option<lhr_render::MeshLighting>>,
    cleanup_failure: Option<String>,
}
/// Each live owner reserves its own failure slot before creating a Scene.
/// Failed cleanup moves the same complete state there, preserving diagnostics.
enum CleanupSlot {
    Vacant,
    Reserved,
    Failed(Box<CanonicalSceneState>),
}
thread_local! {static CLEANUP_STATES:std::cell::RefCell<[CleanupSlot;16]>=std::cell::RefCell::new(std::array::from_fn(|_|CleanupSlot::Vacant));}
pub struct CanonicalSceneOwner {
    state: Option<Box<CanonicalSceneState>>,
    cleanup_slot: usize,
    _thread_bound: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl std::ops::Deref for CanonicalSceneOwner {
    type Target = CanonicalSceneState;
    fn deref(&self) -> &Self::Target {
        self.state.as_deref().expect("live canonical owner state")
    }
}
impl std::ops::DerefMut for CanonicalSceneOwner {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.state
            .as_deref_mut()
            .expect("live canonical owner state")
    }
}
impl Drop for CanonicalSceneOwner {
    fn drop(&mut self) {
        if let Some(mut state) = self.state.take() {
            match state.close() {
                Ok(()) => CLEANUP_STATES
                    .with(|states| states.borrow_mut()[self.cleanup_slot] = CleanupSlot::Vacant),
                Err(error) => {
                    CLEANUP_STATES.with(|states| {
                        states.borrow_mut()[self.cleanup_slot] = CleanupSlot::Failed(state)
                    });
                    eprintln!(
                        "Canonical Scene cleanup failure retained in slot {}: {error}",
                        self.cleanup_slot
                    );
                }
            }
        }
    }
}
/// Read-only inspection of a retained failure, including its actual ModelArena,
/// Scene key, loader frontier and already completed teardown prefix.
pub fn inspect_cleanup_failure<R>(
    slot: usize,
    inspect: impl FnOnce(&CanonicalSceneState) -> R,
) -> Option<R> {
    CLEANUP_STATES.with(|states| match states.borrow().get(slot) {
        Some(CleanupSlot::Failed(state)) => Some(inspect(state)),
        _ => None,
    })
}
pub fn cleanup_failure_slots() -> Vec<usize> {
    CLEANUP_STATES.with(|states| {
        states
            .borrow()
            .iter()
            .enumerate()
            .filter_map(|(i, s)| matches!(s, CleanupSlot::Failed(_)).then_some(i))
            .collect()
    })
}
impl CanonicalSceneState {
    pub fn cleanup_failure(&self) -> Option<&str> {
        self.cleanup_failure.as_deref()
    }
}
pub struct CanonicalSceneSnapshot {
    pub scene: lhr_render::Scene,
    pub frame: SceneRenderFrame,
    pub draw_frames: Vec<crate::frame_transform::FrameKey>,
    pub lighting: Vec<Option<lhr_render::MeshLighting>>,
}
impl CanonicalSceneOwner {
    pub fn open(
        installation: &lhr_assets::Installation,
        mission: &str,
        viewport: [u32; 2],
    ) -> Result<Self, String> {
        if viewport.contains(&0) || viewport.iter().any(|&v| v > 8192) {
            return Err("Canonical scene target extent unsupported".into());
        }
        let state = CanonicalSceneState {
            arena: ModelArena::default(),
            scene: None,
            resources: SceneResources::default(),
            driver: DriverSceneOwner::default(),
            textures: TextureResources::new().map_err(|e| format!("Scene textures: {e:?}"))?,
            storage: CpuTextureStorage::default(),
            world: None,
            loader: None,
            frontier: None,
            caches: MissionCaches::empty().map_err(|e| format!("Scene model cache: {e:?}"))?,
            cache_memory: CpuMissionCacheMemory::default(),
            precision: Default::default(),
            closed: false,
            lighting: Vec::new(),
            cleanup_failure: None,
        };
        let cleanup_slot = CLEANUP_STATES.with(|states| {
            let mut states = states.borrow_mut();
            let slot = states
                .iter()
                .position(|s| matches!(s, CleanupSlot::Vacant))
                .ok_or("Canonical Scene cleanup retention capacity reached")?;
            states[slot] = CleanupSlot::Reserved;
            Ok::<_, String>(slot)
        })?;
        let mut owner = Self {
            state: Some(Box::new(state)),
            cleanup_slot,
            _thread_bound: Default::default(),
        };
        let result = crate::native_precision::Pc24PrecisionState::run_owner(
            owner
                .state
                .as_deref_mut()
                .expect("live canonical owner state"),
            |o| &mut o.precision,
            |o| o.open_inner(installation, mission, viewport),
        );
        if let Err(error) = result {
            let cleanup = owner.close();
            return Err(format!("{error}; canonical scene cleanup: {cleanup:?}"));
        }
        Ok(owner)
    }
    pub fn close(&mut self) -> Result<(), String> {
        self.state
            .as_deref_mut()
            .expect("live canonical owner state")
            .close()
    }
}
impl CanonicalSceneState {
    fn open_inner(
        &mut self,
        installation: &lhr_assets::Installation,
        mission: &str,
        viewport: [u32; 2],
    ) -> Result<(), String> {
        self.arena
            .visual_resource_owner_mut()
            .map_err(|e| e.to_string())?
            .enable_inspection_rgba8_color_key()
            .map_err(|e| format!("Scene texture upload format: {e:?}"))?;
        let created = self.driver.create_scene(
            &mut DriverSceneContext {
                arena: &mut self.arena,
                resources: &mut self.resources,
                textures: &mut self.textures,
                storage: &mut self.storage,
            },
            &mut Screen(viewport),
            &mut Images(installation),
            NativeSceneSoundDriver::Absent,
        );
        let scene = match created {
            Ok(scene) => {
                self.scene = Some(scene);
                scene
            }
            Err(failure) => {
                self.scene = failure.scene;
                return Err(format!("Canonical Scene construction: {failure:?}"));
            }
        };
        match self.driver.load_scene_world_cpu(
            &mut self.arena,
            &self.resources,
            scene,
            installation,
            &format!("missions\\{mission}\\scene.i3d"),
        ) {
            Ok(world) => self.world = Some(world),
            Err(failure) => {
                self.world = Some(failure.progress);
                return Err(format!("Canonical world intake: {:?}", failure.error));
            }
        }
        self.caches
            .add_dir(MissionCacheKind::Model, b"Models", &mut self.cache_memory)
            .map_err(|e| format!("Scene model directory: {e:?}"))?;
        let mut loader = MissionSceneLoader::new(&self.arena, scene).map_err(|e| e.detail)?;
        let bytes = installation
            .read(&format!("missions/{mission}/scene2.bin"))
            .map_err(|e| format!("Canonical scene2 input: {e}"))?;
        // Explicit ordinary host screen mode used by this inspection caller.
        // This is not original GameInit or player-camera scheduling.
        let result = loader.load_prefix_with_model_cache(
            &mut self.driver,
            &mut self.arena,
            &self.resources,
            &mut Screen(viewport),
            crate::game_camera_projection::GameCameraProjection {
                global_mode: 0,
                screen_flag_46f4: false,
            },
            &bytes,
            &mut self.caches,
            installation,
            &mut self.cache_memory,
        );
        self.frontier = result.err();
        self.loader = Some(loader);
        self.arena
            .publish_runtime_buffers()
            .map_err(|e| format!("Canonical scene publication: {e:?}"))?;
        Ok(())
    }
    pub fn camera_control(&mut self) -> Result<lhr_render::Camera, String> {
        let scene = self.scene.ok_or("Canonical Scene closed")?;
        let camera = self
            .arena
            .scene_active_camera(scene)
            .map_err(|e| e.to_string())?
            .ok_or("Canonical Scene has no active camera")?;
        self.arena
            .camera_update(camera)
            .map_err(|e| e.to_string())?;
        let world = self
            .arena
            .world_matrix(camera.frame())
            .map_err(|e| e.to_string())?;
        let reflect = |v: [f32; 3]| glam::Vec3::new(v[0], v[1], -v[2]);
        let eye = reflect(world.elements[12..15].try_into().unwrap());
        let fields = self.arena.camera(camera).map_err(|e| e.to_string())?;
        Ok(lhr_render::Camera {
            eye,
            target: eye + reflect(world.elements[8..11].try_into().unwrap()),
            up: reflect(world.elements[4..7].try_into().unwrap()),
            near: fields.scalar(0x144).ok_or("Camera near range absent")?,
            far: fields.scalar(0x148).ok_or("Camera far range absent")?,
        })
    }
    pub fn set_inspection_camera_pose(
        &mut self,
        control: &lhr_render::Camera,
    ) -> Result<(), String> {
        control.validate()?;
        let scene = self.scene.ok_or("Canonical Scene closed")?;
        let camera = self
            .arena
            .scene_active_camera(scene)
            .map_err(|e| e.to_string())?
            .ok_or("Canonical Scene has no active camera")?;
        self.arena
            .camera_update(camera)
            .map_err(|e| e.to_string())?;
        let current = self
            .arena
            .world_matrix(camera.frame())
            .map_err(|e| e.to_string())?;
        let eye = glam::Vec3::new(
            current.elements[12],
            current.elements[13],
            -current.elements[14],
        );
        let forward = glam::Vec3::new(
            current.elements[8],
            current.elements[9],
            -current.elements[10],
        );
        if control.eye == eye && control.target == eye + forward {
            return Ok(());
        }
        let direction = control.target - control.eye;
        self.arena
            .camera_set_position(camera, [control.eye.x, control.eye.y, -control.eye.z])
            .map_err(|e| e.to_string())?;
        self.arena
            .camera_set_direction(camera, [direction.x, direction.y, -direction.z], 0.)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn snapshot(
        &mut self,
        viewport: [u32; 2],
        render_number: u32,
    ) -> Result<CanonicalSceneSnapshot, String> {
        let mut snapshot = crate::native_precision::Pc24PrecisionState::run_owner(
            self,
            |o| &mut o.precision,
            |o| {
                let scene = o.scene.ok_or("Canonical scene is closed")?;
                o.resources
                    .set_viewport(
                        &o.arena,
                        scene,
                        [0, 0, viewport[0] as i32, viewport[1] as i32],
                    )
                    .map_err(|e| format!("Scene viewport: {e:?}"))?;
                let camera = o
                    .arena
                    .scene_active_camera(scene)
                    .map_err(|e| e.to_string())?
                    .ok_or("Canonical Scene camera absent")?;
                o.arena
                    .camera_set_aspect_ratio(camera, viewport[0] as f32 / viewport[1] as f32)
                    .map_err(|e| e.to_string())?;
                snapshot_scene(
                    &mut o.arena,
                    scene,
                    o.resources
                        .construction(scene)
                        .and_then(|c| c.viewport_projection)
                        .ok_or("Canonical Scene viewport absent")?,
                    viewport,
                    render_number,
                )
            },
        )?;
        self.lighting = std::mem::take(&mut snapshot.lighting);
        Ok(snapshot)
    }
    pub fn upload_lighting(&self, renderer: &mut lhr_render::Renderer) -> Result<(), String> {
        for (i, lighting) in self.lighting.iter().enumerate() {
            if let Some(lighting) = lighting {
                renderer.set_mesh_lighting(i, lighting)?;
            } else {
                renderer.clear_mesh_lighting(i)?;
            }
        }
        Ok(())
    }
    pub fn close(&mut self) -> Result<(), String> {
        if self.closed {
            return Ok(());
        }
        if let Some(error) = &self.cleanup_failure {
            return Err(error.clone());
        }
        let result: Result<(), String> = crate::native_precision::Pc24PrecisionState::run_owner(
            self,
            |o| &mut o.precision,
            |o| {
                if let Some(scene) = o.scene {
                    if let Some(loader) = &mut o.loader {
                        loader
                            .release_pending_visual_replacement(&mut o.arena)
                            .map_err(|e| format!("Pending visual cleanup: {e:?}"))?;
                    }
                    if let Some(world) = &mut o.world {
                        o.driver
                            .discard_scene_world_staging_cpu(&mut o.arena, &o.resources, world)
                            .map_err(|e| format!("World staging cleanup: {e:?}"))?;
                    }
                    // Preserve loader-owned partial states through actual scene close.
                    o.resources
                        .close_constructed_scene(
                            &mut o.arena,
                            scene,
                            &mut o.textures,
                            &mut o.storage,
                        )
                        .map_err(|e| format!("Canonical Scene close: {e:?}"))?;
                    o.driver
                        .release_scene(
                            &mut DriverSceneContext {
                                arena: &mut o.arena,
                                resources: &mut o.resources,
                                textures: &mut o.textures,
                                storage: &mut o.storage,
                            },
                            scene,
                        )
                        .map_err(|e| format!("Canonical Scene release: {e:?}"))?;
                    o.scene = None;
                }
                for kind in [
                    MissionCacheKind::Model,
                    MissionCacheKind::Sound,
                    MissionCacheKind::Animation,
                ] {
                    o.caches
                        .destruct(kind, &mut o.arena, &mut o.cache_memory)
                        .map_err(|e| format!("Canonical cache teardown: {e:?}"))?;
                }
                o.driver
                    .release_empty_registry_storage()
                    .map_err(|e| format!("Canonical driver registry: {e:?}"))?;
                o.arena
                    .reclaim_unused_visual_buffers()
                    .map_err(|e| format!("Canonical visual reclamation: {e:?}"))?;
                o.lighting.clear();
                o.closed = true;
                Ok(())
            },
        );
        if let Err(error) = &result {
            self.cleanup_failure = Some(error.clone());
        }
        result
    }
}
pub fn snapshot_scene(
    arena: &mut ModelArena,
    scene: SceneKey,
    viewport: SceneViewport,
    target: [u32; 2],
    render_number: u32,
) -> Result<CanonicalSceneSnapshot, String> {
    let visible = arena
        .process_scene_visibility(
            scene,
            crate::scene_visibility::SceneVisibilityOptions {
                viewport: target,
                render_number,
                use_occlusion: true,
            },
        )
        .map_err(|e| format!("Canonical visibility: {e:?}"))?;
    let view = active_lod_view(arena, scene, 1.)?;
    let frames: Vec<_> = visible
        .backdrop
        .iter()
        .chain(&visible.main)
        .chain(&visible.near)
        .copied()
        .collect();
    let snapshot = crate::car_inspection::snapshot_selected_frames_with_view(arena, &frames, view)?;
    let indices = |members: &[crate::frame_transform::FrameKey]| {
        snapshot
            .draw_frames
            .iter()
            .enumerate()
            .filter_map(|(i, f)| members.contains(f).then_some(i))
            .collect()
    };
    let draws = ScenePassDraws {
        backdrop: indices(&visible.backdrop),
        main: indices(&visible.main),
        near: indices(&visible.near),
    };
    let (_, frame) = camera_frame(arena, scene, viewport, draws)?;
    Ok(CanonicalSceneSnapshot {
        scene: snapshot.scene,
        frame,
        draw_frames: snapshot.draw_frames,
        lighting: snapshot.lighting,
    })
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;
    struct MissingImages;
    impl TextureImageSource for MissingImages {
        fn read_image(&mut self, _: &[u8]) -> Result<Vec<u8>, TextureImageError> {
            Err(TextureImageError::Missing)
        }
    }
    fn fixture() -> CanonicalSceneOwner {
        let mut state = CanonicalSceneState {
            arena: ModelArena::default(),
            scene: None,
            resources: SceneResources::default(),
            driver: DriverSceneOwner::default(),
            textures: TextureResources::new().unwrap(),
            storage: CpuTextureStorage::default(),
            world: None,
            loader: None,
            frontier: None,
            caches: MissionCaches::empty().unwrap(),
            cache_memory: CpuMissionCacheMemory::default(),
            precision: Default::default(),
            closed: false,
            lighting: Vec::new(),
            cleanup_failure: None,
        };
        let scene = state
            .driver
            .create_scene(
                &mut DriverSceneContext {
                    arena: &mut state.arena,
                    resources: &mut state.resources,
                    textures: &mut state.textures,
                    storage: &mut state.storage,
                },
                &mut Screen([64, 64]),
                &mut MissingImages,
                NativeSceneSoundDriver::Absent,
            )
            .unwrap();
        state.scene = Some(scene);
        let cleanup_slot = CLEANUP_STATES.with(|states| {
            let mut states = states.borrow_mut();
            let slot = states
                .iter()
                .position(|s| matches!(s, CleanupSlot::Vacant))
                .unwrap();
            states[slot] = CleanupSlot::Reserved;
            slot
        });
        CanonicalSceneOwner {
            state: Some(Box::new(state)),
            cleanup_slot,
            _thread_bound: Default::default(),
        }
    }
    #[test]
    fn prospective_scene_early_exit_runs_actual_cleanup_and_releases_reservation() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let owner = fixture();
            assert_eq!(owner.arena.live_frame_count(), 5);
            let slot = owner.cleanup_slot;
            fn rejected_before_transfer(_owner: CanonicalSceneOwner) -> Result<(), String> {
                Err("synthetic graphics initialization failure".into())
            }
            assert!(rejected_before_transfer(owner).is_err());
            CLEANUP_STATES
                .with(|states| assert!(matches!(states.borrow()[slot], CleanupSlot::Vacant)));
        });
    }
    #[test]
    fn failed_cleanup_retains_the_same_scene_and_sticky_prefix_without_replay() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut owner = fixture();
            let scene = owner.scene.unwrap();
            // A genuine outstanding caller reference prevents driver registry
            // destruction after the scene's ordinary close prefix completes.
            owner.arena.retain(scene.root()).unwrap();
            let slot = owner.cleanup_slot;
            let identity = owner.arena.lookup_reference(scene.root());
            let first = owner.close().unwrap_err();
            let frames = owner.arena.live_frame_count();
            assert_eq!(owner.close().unwrap_err(), first);
            assert_eq!(owner.arena.live_frame_count(), frames);
            drop(owner);
            inspect_cleanup_failure(slot, |state| {
                assert_eq!(state.arena.lookup_reference(scene.root()), identity);
                assert_eq!(state.arena.frame(scene.root()).unwrap().references, 1);
                assert!(state.driver.scenes().contains(&scene));
                assert_eq!(state.cleanup_failure(), Some(first.as_str()));
                assert_eq!(state.arena.live_frame_count(), frames);
            })
            .expect("failed owner retained for inspection");
        });
    }
}
