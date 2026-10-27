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
    audio: crate::runtime_audio::RuntimeAudio,
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
        Self::open_at_driver_time(installation, mission, viewport, 0)
    }
    /// Preserve the host driver timeline across scene replacement; material
    /// constructors observe this time before any model/material admission.
    pub fn open_at_driver_time(
        installation: &lhr_assets::Installation,
        mission: &str,
        viewport: [u32; 2],
        material_driver_time: i32,
    ) -> Result<Self, String> {
        if viewport.contains(&0) || viewport.iter().any(|&v| v > 8192) {
            return Err("Canonical scene target extent unsupported".into());
        }
        let mut state = CanonicalSceneState {
            arena: ModelArena::default(),
            scene: None,
            resources: SceneResources::default(),
            driver: DriverSceneOwner::default(),
            textures: TextureResources::new().map_err(|e| format!("Scene textures: {e:?}"))?,
            storage: CpuTextureStorage::default(),
            audio: crate::runtime_audio::RuntimeAudio::default(),
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
        state
            .arena
            .set_material_driver_time(material_driver_time)
            .map_err(|e| e.to_string())?;
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
        self.caches
            .add_dir(MissionCacheKind::Sound, b"Sounds", &mut self.cache_memory)
            .map_err(|e| format!("Scene sound directory: {e:?}"))?;
        self.driver
            .bind_host_sound_backend(&mut self.arena, &self.audio)
            .map_err(|e| format!("Canonical host PCM backend: {e:?}"))?;
        let created = self.driver.create_scene(
            &mut DriverSceneContext {
                arena: &mut self.arena,
                resources: &mut self.resources,
                textures: &mut self.textures,
                storage: &mut self.storage,
            },
            &mut Screen(viewport),
            &mut Images(installation),
            NativeSceneSoundDriver::Initialized,
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
        loader
            .content
            .set_program_context(Some(mission.as_bytes().to_vec()), Vec::new())
            .map_err(|error| format!("Mission program context: {error}"))?;
        let bytes = installation
            .read(&format!("missions/{mission}/scene2.bin"))
            .map_err(|e| format!("Canonical scene2 input: {e}"))?;
        // Explicit ordinary host screen mode used by this inspection caller.
        // This is not original GameInit or player-camera scheduling.
        let result = loader.load_prefix_with_resource_cache(
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
            vertical_fov_radians: 50f32.to_radians(),
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
    /// Lab inspection setting written to the actual active Scene camera.
    /// The canonical snapshot subsequently reads this native camera and the
    /// Scene viewport; the host control Camera never becomes the pass producer.
    pub fn set_inspection_camera_fov(
        &mut self,
        control: &lhr_render::Camera,
        viewport: [u32; 2],
    ) -> Result<(), String> {
        let horizontal = control.horizontal_fov_radians(viewport)?;
        crate::native_precision::Pc24PrecisionState::run_owner(
            self,
            |owner| &mut owner.precision,
            |owner| {
                let scene = owner.scene.ok_or("Canonical Scene closed")?;
                let camera = owner
                    .arena
                    .scene_active_camera(scene)
                    .map_err(|e| e.to_string())?
                    .ok_or("Canonical Scene has no active camera")?;
                owner
                    .arena
                    .camera_set_fov(camera, horizontal)
                    .map_err(|e| e.to_string())
            },
        )
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
                    if o.driver.scenes().contains(&scene) {
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
                    } else {
                        o.resources
                            .release_scene(&mut o.arena, scene, &mut o.textures, &mut o.storage)
                            .map_err(|e| format!("Canonical unregistered Scene release: {e:?}"))?;
                        o.driver
                            .release_unregistered_scene_listener(scene)
                            .map_err(|e| format!("Canonical unregistered listener: {e:?}"))?;
                    }
                    if o.arena.scene(scene).is_err() {
                        o.scene = None;
                    }
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
                o.audio
                    .clear_listener()
                    .map_err(|e| format!("Canonical host PCM listener: {e:?}"))?;
                o.driver
                    .release_empty_registry_storage()
                    .map_err(|e| format!("Canonical driver registry: {e:?}"))?;
                o.arena
                    .reclaim_unused_visual_buffers()
                    .map_err(|e| format!("Canonical visual reclamation: {e:?}"))?;
                let remaining = crate::mission_load_session::MissionOwnedCounts::from_owners(
                    &o.arena,
                    &o.driver,
                    &o.resources,
                    &o.textures,
                    &o.storage,
                    &o.cache_memory,
                    &o.audio,
                    o.loader.as_ref(),
                )
                .map_err(|e| format!("Canonical owner diagnostics: {e:?}"))?;
                if remaining != Default::default() {
                    return Err(format!(
                        "Canonical owners remain after Close: {remaining:?}"
                    ));
                }
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
    let camera = arena
        .scene_active_camera(scene)
        .map_err(|e| e.to_string())?
        .ok_or("Canonical Scene has no active camera")?;
    // CullSector still processes the backdrop even inside a closed interior.
    // Render's later AddSectorFGroups gate reads the PRIMARY sector's visible
    // bit, not the backdrop's CULLED bit (render.cpp, mafia-re eb5a921).
    let primary = arena.scene(scene).map_err(|e| e.to_string())?.sectors[0];
    let draw_backdrop = arena
        .sector(primary)
        .map_err(|e| e.to_string())?
        .sector_flags
        & crate::scene_visibility::VISIBLE
        != 0;
    let backdrop = if draw_backdrop {
        visible.backdrop.as_slice()
    } else {
        &[]
    };
    let frames: Vec<_> = backdrop
        .iter()
        .chain(&visible.main)
        .chain(&visible.near)
        .copied()
        .collect();
    let snapshot = crate::car_inspection::snapshot_selected_frames_with_view_and_camera(
        arena, &frames, view, camera,
    )?;
    let indices = |members: &[crate::frame_transform::FrameKey]| {
        snapshot
            .draw_frames
            .iter()
            .enumerate()
            .filter_map(|(i, f)| members.contains(f).then_some(i))
            .collect()
    };
    let draws = ScenePassDraws {
        backdrop: indices(backdrop),
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
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
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
            audio: crate::runtime_audio::RuntimeAudio::default(),
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
        state
            .driver
            .bind_host_sound_backend(&mut state.arena, &state.audio)
            .unwrap();
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
                NativeSceneSoundDriver::Initialized,
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
    #[test]
    fn canonical_window_scene_opens_sound_with_same_live_owner_and_closes_it() {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let path = std::env::temp_dir().join(format!(
            "lhr-canonical-sound-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(path.join("missions/synthetic")).unwrap();
        std::fs::create_dir_all(path.join("Sounds")).unwrap();
        let mut world = b"4DS\0".to_vec();
        world.extend(29u16.to_le_bytes());
        world.extend([0; 8]);
        world.extend(0u16.to_le_bytes()); // materials
        world.extend(0u16.to_le_bytes()); // frames
        world.push(0); // animations
        std::fs::write(path.join("missions/synthetic/scene.4ds"), world).unwrap();
        let chunk = |kind: u16, payload: &[u8]| -> Vec<u8> {
            [
                kind.to_le_bytes().as_slice(),
                &u32::try_from(payload.len() + 6).unwrap().to_le_bytes(),
                payload,
            ]
            .concat()
        };
        let filename = chunk(0x10, b"a.wav\0");
        let sound_fields = chunk(0x4060, &filename);
        let sound = chunk(
            0x4010,
            &[
                chunk(0x4011, &4u32.to_le_bytes()),
                chunk(0x10, b"source sound\0"),
                sound_fields,
            ]
            .concat(),
        );
        let scene2 = chunk(
            0x4c53,
            &[chunk(1, &[5, 0, 1, 0]), chunk(0x4000, &sound)].concat(),
        );
        std::fs::write(path.join("missions/synthetic/scene2.bin"), scene2).unwrap();
        let mut wav = b"RIFF".to_vec();
        wav.extend(44u32.to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend(1u16.to_le_bytes()); // PCM
        wav.extend(1u16.to_le_bytes()); // mono
        wav.extend(22_050u32.to_le_bytes());
        wav.extend(44_100u32.to_le_bytes());
        wav.extend(2u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(8u32.to_le_bytes());
        wav.extend([0; 8]);
        std::fs::write(path.join("Sounds/a.wav"), wav).unwrap();
        let installation = lhr_assets::Installation::open(&path).unwrap();
        let mut owner = CanonicalSceneOwner::open(&installation, "synthetic", [800, 600]).unwrap();
        assert!(owner.frontier.is_none(), "{:?}", owner.frontier);
        assert_eq!(owner.caches.len(MissionCacheKind::Sound), 1);
        assert_eq!(owner.audio.live_sound_count().unwrap(), 2);
        assert_eq!(owner.audio.live_voice_count().unwrap(), 2);
        assert_eq!(owner.audio.live_source_count().unwrap(), 1);
        assert_eq!(
            owner
                .driver
                .host_sound_backend()
                .unwrap()
                .listeners()
                .unwrap()
                .len(),
            1
        );
        let scene = owner.scene.unwrap();
        owner.close().unwrap();
        assert_eq!(owner.audio.live_sound_count().unwrap(), 0);
        assert_eq!(
            owner
                .driver
                .host_sound_backend()
                .unwrap()
                .listeners()
                .unwrap()
                .len(),
            0
        );
        assert!(owner.resources.cpu_close_report(scene).is_some());
        drop(owner);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn canonical_billboard_draw_uses_live_camera_and_selected_fade_without_moving_frame() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let path = std::env::temp_dir().join(format!(
                "lhr-billboard-consumer-{}-{}",
                std::process::id(),
                crate::frame_lookup::incarnation()
            ));
            std::fs::create_dir(&path).unwrap();
            let installation = lhr_assets::Installation::open(&path).unwrap();
            let mut owner = fixture();
            let scene = owner.scene.unwrap();
            let primary_sector = owner.arena.scene(scene).unwrap().sectors[0];
            let primary = primary_sector.frame();
            let model = owner.arena.create_model().unwrap();
            owner.arena.scene_add_frame(scene, model.root()).unwrap();
            owner
                .arena
                .link_frame(model.root(), Some(primary), 0)
                .unwrap();
            let mut visual = crate::model_loading::tests::frame(1, None, [0., 0., 7.5]);
            visual.visual = Some(4);
            visual.render_flags = 0;
            visual.logic_flags = 0;
            for distance in [16., 64.] {
                visual.lods.push(lhr_assets::Lod {
                    distance,
                    uv_only: None,
                    vertices: [[-1., -1., 0.], [1., -1., 0.], [0., 1., 0.]]
                        .map(|position| lhr_assets::Vertex {
                            position,
                            normal: [0., 0., -1.],
                            uv: [0.; 2],
                        })
                        .to_vec(),
                    groups: vec![
                        lhr_assets::FaceGroup {
                            material: 0,
                            indices: vec![0, 1, 2],
                        },
                        lhr_assets::FaceGroup {
                            material: lhr_assets::UNASSIGNED_MATERIAL,
                            indices: vec![0, 1, 2],
                        },
                        lhr_assets::FaceGroup {
                            material: 1,
                            indices: vec![0, 1, 2],
                        },
                        lhr_assets::FaceGroup {
                            material: 2,
                            indices: vec![0, 1, 2],
                        },
                        lhr_assets::FaceGroup {
                            material: 3,
                            indices: vec![0, 1, 2],
                        },
                    ],
                });
            }
            let first_material = lhr_assets::Material {
                flags: 0,
                ambient: [0.; 3],
                diffuse: [1.; 3],
                texture: String::new(),
                emission: [0.; 3],
                opacity: 0.5,
                alpha_texture: None,
                reflection: None,
                environment_texture: None,
                animation: None,
            };
            let mut zero_diffuse = first_material.clone();
            zero_diffuse.diffuse = [0., 1., 1.];
            zero_diffuse.emission = [0.25, 0., 0.];
            let mut all_nonzero = zero_diffuse.clone();
            all_nonzero.diffuse = [1.; 3];
            let mut negative_zero = zero_diffuse.clone();
            negative_zero.diffuse[0] = -0.;
            let source = Arc::new(lhr_assets::Model {
                materials: vec![first_material, zero_diffuse, all_nonzero, negative_zero],
                frames: vec![visual],
            });
            let loaded = owner
                .arena
                .load_parsed_billboards_into_model(
                    model,
                    source,
                    &[lhr_assets::BillboardSource {
                        frame_index: 0,
                        rot_axis_word: 2,
                        axis_mode_byte: 0,
                    }],
                )
                .unwrap();
            let frame = loaded.frames()[0];
            owner
                .arena
                .load_plain_visual_resources(model, &installation)
                .unwrap();
            owner.arena.release(model.root()).unwrap();
            let object = owner.arena.frame_object_resources(frame).unwrap().unwrap();
            let owned = owner.arena.loaded_visual_resources().unwrap().objects();
            let mesh = owned.object(object).unwrap().mesh.unwrap();
            let managed = owned.managed().resources();
            let level = managed.mesh_object(mesh).unwrap().levels[1].unwrap();
            let group = managed.mesh_level(level).unwrap().fgroups.unwrap();
            let material = managed.face_group(group).unwrap().material.unwrap();
            let world_before = owner.arena.world_matrix(frame).unwrap();
            let no_camera = crate::car_inspection::snapshot_selected_frames_with_view(
                &mut owner.arena,
                &[frame],
                crate::inspection_lod::InspectionLodView {
                    camera_pc: [0.; 3],
                    rs_lod_quality: 1.,
                },
            )
            .err()
            .unwrap();
            assert!(no_camera.contains("Billboard requires active Scene camera"));
            let a = owner.arena.create_camera(&mut Screen([64, 64])).unwrap();
            let b = owner.arena.create_camera(&mut Screen([64, 64])).unwrap();
            let c = owner.arena.create_camera(&mut Screen([64, 64])).unwrap();
            for camera in [a, b, c] {
                owner.arena.camera_set_range(camera, 0.1, 100.).unwrap();
            }
            owner.arena.camera_set_position(b, [1., 0., 0.]).unwrap();
            owner.arena.camera_set_position(c, [0., 0., 2.5]).unwrap();
            let draw = |owner: &mut CanonicalSceneOwner, camera| {
                owner
                    .arena
                    .scene_set_active_camera(scene, Some(camera))
                    .unwrap();
                let active = owner.arena.scene_active_camera(scene).unwrap().unwrap();
                assert_eq!(active, camera);
                let position = owner.arena.world_matrix(active.frame()).unwrap().elements[12..15]
                    .try_into()
                    .unwrap();
                let view = crate::inspection_lod::InspectionLodView {
                    camera_pc: position,
                    rs_lod_quality: 1.,
                };
                let bound = owner.arena.visual_world_bounds(frame).unwrap();
                let distance_squared = view.distance_squared(bound.center).unwrap();
                assert!((16. ..64.).contains(&distance_squared));
                let selected =
                    crate::car_inspection::snapshot_selected_frames_with_view_and_camera(
                        &mut owner.arena,
                        &[frame],
                        view,
                        active,
                    )
                    .unwrap();
                assert_eq!(selected.draw_frames, [frame; 5]);
                let live = owner
                    .arena
                    .loaded_visual_resources()
                    .unwrap()
                    .objects()
                    .object(object)
                    .unwrap();
                assert_eq!(live.cur_lod, 1);
                let expected = (1. - (distance_squared - 48.) / 16.).min(1.);
                assert!((live.fade - expected).abs() < 1e-5);
                let lhr_render::RenderMaterial::Original(material) =
                    selected.scene.meshes[0].material
                else {
                    panic!("billboard must use its selected source material");
                };
                assert_eq!(&material.color[..3], &[1.; 3]);
                let colors = selected.lighting[0].as_ref().unwrap().colors.clone();
                let packed_alpha = (expected * 0.5 * 255.).round_ties_even() / 255.;
                assert!(
                    (material.color[3] - packed_alpha).abs() < 1e-5,
                    "assigned material alpha must use native packed TempAlpha"
                );
                assert_eq!(material.color[3], colors[0][3]);
                if expected < 1. {
                    assert_ne!(material.color[3], expected * 0.5);
                } else {
                    assert_eq!(material.color[3], 128. / 255.);
                }
                let lhr_render::RenderMaterial::OriginalNull { color, .. } =
                    selected.scene.meshes[1].material
                else {
                    panic!("billboard's unassigned face group must keep null policy");
                };
                assert_eq!(color[3], packed_alpha);
                assert_eq!(
                    selected.lighting[1].as_ref().unwrap().colors[0][3],
                    color[3]
                );
                for (index, expected_alpha) in [
                    (2, lhr_render::AlphaMode::Blend),
                    (3, lhr_render::AlphaMode::Additive),
                    (4, lhr_render::AlphaMode::Additive),
                ] {
                    let lhr_render::RenderMaterial::Original(later) =
                        selected.scene.meshes[index].material
                    else {
                        panic!("later billboard group must retain assigned policy");
                    };
                    assert_eq!(later.alpha, expected_alpha);
                    assert_eq!(later.color[3], packed_alpha);
                    assert_eq!(selected.lighting[index].as_ref().unwrap().colors, colors);
                }
                assert_eq!(colors.len(), selected.scene.meshes[0].vertices.len());
                let positions = selected.scene.meshes[0]
                    .vertices
                    .iter()
                    .map(|vertex| vertex.position)
                    .collect::<Vec<_>>();
                (positions, colors)
            };
            let first = draw(&mut owner, a);
            let second = draw(&mut owner, b);
            let full_fade = draw(&mut owner, c);
            assert_ne!(first.0, second.0);
            assert_ne!(full_fade.0, second.0);
            assert_eq!(owner.arena.world_matrix(frame).unwrap(), world_before);
            // VISF_WORLD_SPACE changes the native branch and is explicitly
            // outside this static local billboard presentation subset.
            let flags = owner.arena.state(frame).unwrap().visual_flags;
            owner
                .arena
                .graph
                .projected_state_mut(frame)
                .unwrap()
                .visual_flags |= 0x40;
            let world_space = crate::car_inspection::snapshot_selected_frames_with_view_and_camera(
                &mut owner.arena,
                &[frame],
                crate::inspection_lod::InspectionLodView {
                    camera_pc: [1., 0., 0.],
                    rs_lod_quality: 1.,
                },
                b,
            )
            .err()
            .unwrap();
            assert!(world_space.contains("world-space/proxy domain unavailable"));
            owner
                .arena
                .graph
                .projected_state_mut(frame)
                .unwrap()
                .visual_flags = flags;
            // A contributing directional light needs the billboard draw
            // matrix inverse for vertex normals; the unlit bridge refuses it.
            let light = owner.arena.create_light().unwrap();
            owner.arena.graph.set_light_type(light, 3).unwrap();
            owner.arena.graph.set_light_mode(light, 1).unwrap();
            owner.arena.graph.set_light_diffuse(light, [1.; 3]).unwrap();
            owner
                .arena
                .graph
                .set_light_diffuse_power(light, 1.)
                .unwrap();
            owner
                .arena
                .graph
                .assign_rotation(
                    light,
                    crate::native_transform::SQuat {
                        wxyz: [0., 0., 1., 0.],
                    },
                )
                .unwrap();
            owner.arena.scene_add_frame(scene, light).unwrap();
            owner.arena.sector_add_light(primary_sector, light).unwrap();
            owner.arena.update_frame(light).unwrap();
            owner.arena.release(light).unwrap();
            let lit_first = draw(&mut owner, a);
            let lit_second = draw(&mut owner, b);
            assert_ne!(lit_first.1, lit_second.1);
            owner.arena.release(a.frame()).unwrap();
            owner.arena.release(b.frame()).unwrap();
            owner.arena.release(c.frame()).unwrap();
            owner.close().unwrap();
            assert_eq!(owner.arena.live_frame_count(), 0);
            let resources = owner.arena.loaded_visual_resources().unwrap();
            assert_eq!(resources.backend().live_graphics(), 0);
            assert_eq!(resources.backend().live_textures(), 0);
            assert_eq!(resources.backend().live_allocations(), 0);
            let owned = resources.objects();
            assert!(owned.object(object).is_err());
            let managed = owned.managed().resources();
            assert!(managed.mesh_object(mesh).is_err());
            assert!(managed.mesh_level(level).is_err());
            assert!(managed.face_group(group).is_err());
            assert!(managed.material(material).is_err());
            std::fs::remove_dir_all(path).unwrap();
        });
    }
}
