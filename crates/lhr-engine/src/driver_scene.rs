//! Original I3D_driver::CreateFrame (PC 0x193e0), type13 Scene arm only.
//! LS3DF SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! Registry defaults follow driver ctor0x15620; list teardown is only the
//! scene-list member effect of dtor0x15670, not the whole driver destructor.
//!
//! The driver list borrows scenes: append does not AddRef, and removal happens
//! in the scene destructor after shared weather releases, before array/base
//! destruction. Other frame-factory arms and present native audio drivers
//! are explicit dependencies, not successful substitutes.
use crate::itexture::{CpuTextureStorage, TextureImageSource, TextureResources};
use crate::model_loading::{ModelArena, ModelError};
use crate::scene_ownership::SceneKey;
use crate::scene_resources::{
    SceneConstructionError, SceneResourceError, SceneResources, SceneScreen,
};
use crate::script_scheduler::FrameReferenceId;
use std::sync::atomic::{AtomicU64, Ordering};

pub trait DriverSceneScreen: SceneScreen {
    /// Actual IGraph viewport boundaries [left, top, right, bottom].
    fn viewport_rectangle(&mut self) -> Result<[i32; 4], Self::Error>;
}
/// Native I3D sound-driver state, distinct from the host CPU audio mixer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeSceneSoundDriver {
    Absent,
    Uninitialized,
    /// The present driver's IsInitialized/CreateListener callbacks require
    /// their actual native backend, including when initialization is false.
    Initialized,
}
/// Borrows the existing canonical owners; the factory never creates parallel
/// frame, weather or texture domains behind the caller's back.
pub struct DriverSceneContext<'a> {
    pub arena: &'a mut ModelArena,
    pub resources: &'a mut SceneResources,
    pub textures: &'a mut TextureResources,
    pub storage: &'a mut CpuTextureStorage,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DriverSceneCreatePhase {
    Constructor,
    Viewport,
    Listener,
    Registration,
}
#[derive(Debug)]
pub enum DriverSceneError<E> {
    Model(ModelError),
    Constructor(SceneConstructionError<E>),
    Screen(E),
    Resources(SceneResourceError),
    ForeignDomain,
    AllocationIncomplete,
    AllocationUnavailable,
    NativeSoundDriverUnavailable,
    RegistryNotEmpty,
    UnknownScene(SceneKey),
}
#[derive(Debug)]
pub struct DriverSceneCreateFailure<E> {
    pub scene: Option<SceneKey>,
    pub phase: DriverSceneCreatePhase,
    pub error: DriverSceneError<E>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DriverDomains {
    arena: FrameReferenceId,
    resources: u64,
    textures: u64,
    storage: u64,
}
impl DriverDomains {
    fn of(context: &DriverSceneContext<'_>) -> Self {
        Self {
            arena: context
                .arena
                .lookup_reference(crate::frame_transform::FrameKey(0)),
            resources: context.resources.identity(),
            textures: context.textures.objects().managed().resources().identity(),
            storage: context.storage.identity(),
        }
    }
}
/// Borrowed native driver scene registry (+0x48/+0x4c/+0x50/+0x54).
/// Explicit scene releases are required: Rust Drop cannot invoke resource backends.
pub struct DriverSceneOwner {
    identity: u64,
    domains: Option<DriverDomains>,
    scenes: Vec<SceneKey>,
    native_capacity: u32,
    growth: u32,
    allocation_incomplete: bool,
}
impl Default for DriverSceneOwner {
    fn default() -> Self {
        static NEXT_DRIVER: AtomicU64 = AtomicU64::new(1);
        Self {
            identity: NEXT_DRIVER
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                    value.checked_add(1)
                })
                .expect("driver identity exhausted"),
            domains: None,
            scenes: Vec::new(),
            native_capacity: 0,
            growth: 32,
            allocation_incomplete: false,
        }
    }
}
impl DriverSceneOwner {
    pub fn scenes(&self) -> &[SceneKey] {
        &self.scenes
    }
    pub fn capacity(&self) -> u32 {
        self.native_capacity
    }
    pub fn growth(&self) -> u32 {
        self.growth
    }
    pub fn allocation_incomplete(&self) -> bool {
        self.allocation_incomplete
    }
    fn bind<E>(&mut self, context: &mut DriverSceneContext<'_>) -> Result<(), DriverSceneError<E>> {
        let domain = DriverDomains::of(context);
        if self.domains.is_some_and(|existing| existing != domain) {
            return Err(DriverSceneError::ForeignDomain);
        }
        context
            .resources
            .bind_driver_domain(self.identity)
            .map_err(DriverSceneError::Resources)?;
        self.domains = Some(domain);
        if self.allocation_incomplete {
            return Err(DriverSceneError::AllocationIncomplete);
        }
        Ok(())
    }
    /// Executes only original CreateFrame's scene arm. A host allocation or
    /// dependency failure preserves its executed prefix; no native null-return
    /// success or invented constructor rollback is introduced.
    pub fn create_scene<S: DriverSceneScreen>(
        &mut self,
        context: &mut DriverSceneContext<'_>,
        screen: &mut S,
        images: &mut impl TextureImageSource,
        sound: NativeSceneSoundDriver,
    ) -> Result<SceneKey, DriverSceneCreateFailure<S::Error>> {
        self.create_scene_with_allocator(context, screen, images, sound, |_, _| Ok(()))
    }
    /// Allocation callback observes the native already-grown capacity. This
    /// host fault-injection boundary is not an original error-handler result.
    pub fn create_scene_with_allocator<S: DriverSceneScreen>(
        &mut self,
        context: &mut DriverSceneContext<'_>,
        screen: &mut S,
        images: &mut impl TextureImageSource,
        sound: NativeSceneSoundDriver,
        mut allocate: impl FnMut(u32, &[SceneKey]) -> Result<(), ()>,
    ) -> Result<SceneKey, DriverSceneCreateFailure<S::Error>> {
        let failure = |scene, phase, error| DriverSceneCreateFailure {
            scene,
            phase,
            error,
        };
        self.bind(context)
            .map_err(|error| failure(None, DriverSceneCreatePhase::Constructor, error))?;
        let scene = context
            .arena
            .construct_scene_owner_prefix()
            .map_err(|error| {
                failure(
                    None,
                    DriverSceneCreatePhase::Constructor,
                    DriverSceneError::Model(error),
                )
            })?;
        context
            .resources
            .continue_constructor(
                context.arena,
                scene,
                screen,
                context.textures,
                images,
                context.storage,
            )
            .map_err(|error| {
                failure(
                    Some(scene),
                    DriverSceneCreatePhase::Constructor,
                    DriverSceneError::Constructor(error),
                )
            })?;
        let [left, top, right, bottom] = screen.viewport_rectangle().map_err(|error| {
            failure(
                Some(scene),
                DriverSceneCreatePhase::Viewport,
                DriverSceneError::Screen(error),
            )
        })?;
        // Caller subtracts bounds using wrapping integer SUB. SetViewport then
        // stores these arguments directly; it does not repair the rectangle.
        let _native_result = context
            .resources
            .set_viewport(
                context.arena,
                scene,
                [
                    left,
                    top,
                    right.wrapping_sub(left),
                    bottom.wrapping_sub(top),
                ],
            )
            .map_err(|error| {
                failure(
                    Some(scene),
                    DriverSceneCreatePhase::Viewport,
                    DriverSceneError::Resources(error),
                )
            })?;
        if sound != NativeSceneSoundDriver::Absent {
            return Err(failure(
                Some(scene),
                DriverSceneCreatePhase::Listener,
                DriverSceneError::NativeSoundDriverUnavailable,
            ));
        }
        if self.scenes.len() >= self.native_capacity as usize {
            self.native_capacity = self.native_capacity.wrapping_add(self.growth);
            self.allocation_incomplete = true;
            allocate(self.native_capacity, &self.scenes).map_err(|_| {
                failure(
                    Some(scene),
                    DriverSceneCreatePhase::Registration,
                    DriverSceneError::AllocationUnavailable,
                )
            })?;
            let additional = (self.native_capacity as usize)
                .checked_sub(self.scenes.len())
                .ok_or_else(|| {
                    failure(
                        Some(scene),
                        DriverSceneCreatePhase::Registration,
                        DriverSceneError::AllocationUnavailable,
                    )
                })?;
            self.scenes.try_reserve_exact(additional).map_err(|_| {
                failure(
                    Some(scene),
                    DriverSceneCreatePhase::Registration,
                    DriverSceneError::AllocationUnavailable,
                )
            })?;
            self.allocation_incomplete = false;
        }
        context
            .resources
            .bind_driver_scene(scene, self.identity)
            .map_err(|error| {
                failure(
                    Some(scene),
                    DriverSceneCreatePhase::Registration,
                    DriverSceneError::Resources(error),
                )
            })?;
        self.scenes.push(scene);
        Ok(scene)
    }
    /// Release the scene-list member storage at the evidenced driver destructor
    /// 0x15670 boundary. The original does not release its borrowed scenes.
    /// Empty-registry admission is a host ownership guard, not a native check.
    /// This does not claim FreeClipArrays or the other driver members.
    pub fn release_empty_registry_storage(
        &mut self,
    ) -> Result<(), DriverSceneError<std::convert::Infallible>> {
        if self.allocation_incomplete {
            return Err(DriverSceneError::AllocationIncomplete);
        }
        if !self.scenes.is_empty() {
            return Err(DriverSceneError::RegistryNotEmpty);
        }
        self.scenes = Vec::new();
        self.native_capacity = 0;
        Ok(())
    }
    /// Release the caller's actual reference. External references preserve both
    /// scene resources and borrowed registry membership until the last release.
    pub fn release_scene(
        &mut self,
        context: &mut DriverSceneContext<'_>,
        scene: SceneKey,
    ) -> Result<(), DriverSceneError<std::convert::Infallible>> {
        self.bind(context)?;
        if !self.scenes.contains(&scene) {
            return Err(DriverSceneError::UnknownScene(scene));
        }
        let identity = self.identity;
        context
            .resources
            .release_driver_scene(
                context.arena,
                scene,
                context.textures,
                context.storage,
                identity,
                &mut |scene| {
                    let index = self
                        .scenes
                        .iter()
                        .position(|candidate| *candidate == scene)
                        .ok_or(SceneResourceError::UnsupportedDomain(
                            "missing driver scene registry entry",
                        ))?;
                    self.scenes.swap_remove(index);
                    Ok(())
                },
            )
            .map_err(DriverSceneError::Resources)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::itexture::TextureImageError;
    use crate::native_precision::Pc24PrecisionState;
    struct Screen {
        calls: Vec<&'static str>,
        rectangle: [i32; 4],
    }
    impl SceneScreen for Screen {
        type Error = ();
        fn screen_sy(&mut self) -> Result<i32, ()> {
            self.calls.push("sy");
            Ok(600)
        }
        fn screen_sx(&mut self) -> Result<i32, ()> {
            self.calls.push("sx");
            Ok(800)
        }
    }
    impl DriverSceneScreen for Screen {
        fn viewport_rectangle(&mut self) -> Result<[i32; 4], ()> {
            self.calls.push("viewport");
            Ok(self.rectangle)
        }
    }
    struct MissingImages;
    impl TextureImageSource for MissingImages {
        fn read_image(&mut self, _: &[u8]) -> Result<Vec<u8>, TextureImageError> {
            Err(TextureImageError::Missing)
        }
    }
    fn screen() -> Screen {
        Screen {
            calls: Vec::new(),
            rectangle: [10, 20, 810, 620],
        }
    }
    #[test]
    fn factory_uses_actual_viewport_arguments_and_registry_borrows_the_scene() {
        Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let mut resources = SceneResources::default();
            let mut textures = TextureResources::new().unwrap();
            let mut storage = CpuTextureStorage::default();
            let mut driver = DriverSceneOwner::default();
            let mut screen = screen();
            let mut context = DriverSceneContext {
                arena: &mut arena,
                resources: &mut resources,
                textures: &mut textures,
                storage: &mut storage,
            };
            let scene = driver
                .create_scene(
                    &mut context,
                    &mut screen,
                    &mut MissingImages,
                    NativeSceneSoundDriver::Absent,
                )
                .unwrap();
            assert_eq!(screen.calls, ["sy", "sx", "viewport"]);
            let viewport = context
                .resources
                .construction(scene)
                .unwrap()
                .viewport_projection
                .unwrap();
            assert_eq!(viewport.rectangle, [10, 20, 800, 600]);
            assert_eq!(viewport.half_extent, [395., -290.]);
            assert_eq!(viewport.center, [405., 310.]);
            assert_eq!(context.arena.frame(scene.root()).unwrap().references, 1);
            assert_eq!(driver.scenes(), [scene]);
            assert_eq!((driver.capacity(), driver.growth()), (32, 32));
            assert!(matches!(
                context.resources.release_scene(
                    context.arena,
                    scene,
                    context.textures,
                    context.storage
                ),
                Err(SceneResourceError::UnsupportedDomain(_))
            ));
            assert_eq!(context.resources.ws_ref_count(), 1);
            driver.release_scene(&mut context, scene).unwrap();
            assert!(driver.scenes().is_empty());
            assert_eq!(driver.capacity(), 32);
            driver.release_empty_registry_storage().unwrap();
            assert_eq!(driver.capacity(), 0);
        });
    }
    #[test]
    fn external_scene_reference_preserves_registration_and_last_release_swap_removes() {
        Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let mut resources = SceneResources::default();
            let mut textures = TextureResources::new().unwrap();
            let mut storage = CpuTextureStorage::default();
            let mut driver = DriverSceneOwner::default();
            let mut context = DriverSceneContext {
                arena: &mut arena,
                resources: &mut resources,
                textures: &mut textures,
                storage: &mut storage,
            };
            let a = driver
                .create_scene(
                    &mut context,
                    &mut screen(),
                    &mut MissingImages,
                    NativeSceneSoundDriver::Absent,
                )
                .unwrap();
            let b = driver
                .create_scene(
                    &mut context,
                    &mut screen(),
                    &mut MissingImages,
                    NativeSceneSoundDriver::Absent,
                )
                .unwrap();
            let c = driver
                .create_scene(
                    &mut context,
                    &mut screen(),
                    &mut MissingImages,
                    NativeSceneSoundDriver::Absent,
                )
                .unwrap();
            context.arena.retain(b.root()).unwrap();
            driver.release_scene(&mut context, b).unwrap();
            assert_eq!(driver.scenes(), [a, b, c]);
            assert_eq!(context.resources.ws_ref_count(), 3);
            driver.release_scene(&mut context, a).unwrap();
            assert_eq!(driver.scenes(), [c, b]);
            driver.release_scene(&mut context, b).unwrap();
            assert_eq!(driver.scenes(), [c]);
            assert!(matches!(
                driver.release_empty_registry_storage(),
                Err(DriverSceneError::RegistryNotEmpty)
            ));
            driver.release_scene(&mut context, c).unwrap();
            assert_eq!(context.resources.ws_ref_count(), 0);
            driver.release_empty_registry_storage().unwrap();
            assert_eq!(driver.growth(), 32);
        });
    }
    #[test]
    fn registry_growth_keeps_old_entries_and_capacity_survives_scene_destructors() {
        Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let mut resources = SceneResources::default();
            let mut textures = TextureResources::new().unwrap();
            let mut storage = CpuTextureStorage::default();
            let mut driver = DriverSceneOwner::default();
            let mut context = DriverSceneContext {
                arena: &mut arena,
                resources: &mut resources,
                textures: &mut textures,
                storage: &mut storage,
            };
            let mut scenes = Vec::new();
            for _ in 0..33 {
                scenes.push(
                    driver
                        .create_scene(
                            &mut context,
                            &mut screen(),
                            &mut MissingImages,
                            NativeSceneSoundDriver::Absent,
                        )
                        .unwrap(),
                );
            }
            assert_eq!(driver.scenes(), scenes);
            assert_eq!(driver.capacity(), 64);
            for scene in scenes {
                driver.release_scene(&mut context, scene).unwrap();
            }
            assert_eq!(driver.capacity(), 64);
            assert!(driver.scenes().is_empty());
            driver.release_empty_registry_storage().unwrap();
            assert_eq!(driver.capacity(), 0);
        });
    }
    #[test]
    fn present_native_audio_and_allocation_failure_preserve_the_executed_prefix() {
        Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let mut resources = SceneResources::default();
            let mut textures = TextureResources::new().unwrap();
            let mut storage = CpuTextureStorage::default();
            let mut driver = DriverSceneOwner::default();
            let mut context = DriverSceneContext {
                arena: &mut arena,
                resources: &mut resources,
                textures: &mut textures,
                storage: &mut storage,
            };
            for sound in [
                NativeSceneSoundDriver::Initialized,
                NativeSceneSoundDriver::Uninitialized,
            ] {
                let failure = driver
                    .create_scene(&mut context, &mut screen(), &mut MissingImages, sound)
                    .unwrap_err();
                assert_eq!(failure.phase, DriverSceneCreatePhase::Listener);
                assert!(matches!(
                    failure.error,
                    DriverSceneError::NativeSoundDriverUnavailable
                ));
                let scene = failure.scene.unwrap();
                assert_eq!(
                    context.resources.construction(scene).unwrap().viewport,
                    [10, 20, 800, 600]
                );
                assert!(driver.scenes().is_empty());
                assert_eq!(context.resources.ws_ref_count(), 1);
                context
                    .resources
                    .release_scene(context.arena, scene, context.textures, context.storage)
                    .unwrap();
            }
            let failure = driver
                .create_scene_with_allocator(
                    &mut context,
                    &mut screen(),
                    &mut MissingImages,
                    NativeSceneSoundDriver::Absent,
                    |capacity, entries| {
                        assert_eq!(capacity, 32);
                        assert!(entries.is_empty());
                        Err(())
                    },
                )
                .unwrap_err();
            assert_eq!(failure.phase, DriverSceneCreatePhase::Registration);
            assert!(driver.allocation_incomplete());
            assert_eq!(driver.capacity(), 32);
            assert!(driver.scenes().is_empty());
            context
                .resources
                .release_scene(
                    context.arena,
                    failure.scene.unwrap(),
                    context.textures,
                    context.storage,
                )
                .unwrap();
            assert!(matches!(
                driver.release_empty_registry_storage(),
                Err(DriverSceneError::AllocationIncomplete)
            ));
        });
    }
    #[test]
    fn setter_preserves_unsigned_width_signed_height_and_exact_integer_loads() {
        let mut arena = ModelArena::default();
        let mut resources = SceneResources::default();
        let mut textures = TextureResources::new().unwrap();
        let mut storage = CpuTextureStorage::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        Pc24PrecisionState::default()
            .run(|| {
                resources.continue_constructor(
                    &arena,
                    scene,
                    &mut screen(),
                    &mut textures,
                    &mut MissingImages,
                    &mut storage,
                )
            })
            .unwrap();
        let mut precision = Pc24PrecisionState::default();
        let result = precision.run(|| {
            resources.set_viewport(
                &arena,
                scene,
                [16_777_217, 16_777_217, 16_777_219, 16_777_219],
            )
        });
        assert_eq!(result.unwrap(), 0);
        let viewport = resources
            .construction(scene)
            .unwrap()
            .viewport_projection
            .unwrap();
        assert_eq!(viewport.half_extent, [1., -1.]);
        assert_eq!(viewport.center, [16_777_218., 16_777_218.]);
        assert!(!precision.precision_raised());
        assert_eq!(
            resources
                .set_viewport(&arena, scene, [1, i32::MAX, 0, i32::MIN])
                .unwrap(),
            0
        );
        let viewport = resources
            .construction(scene)
            .unwrap()
            .viewport_projection
            .unwrap();
        assert_eq!(viewport.rectangle, [1, i32::MAX, 0, i32::MIN]);
        assert_eq!(viewport.half_extent, [2_147_483_648., -0.5]);
        assert_eq!(viewport.center, [2_147_483_648., 2_147_483_648.]);
        resources
            .release_scene(&mut arena, scene, &mut textures, &mut storage)
            .unwrap();
    }
    #[test]
    fn different_shared_resource_owner_is_rejected_before_any_new_scene_effect() {
        Pc24PrecisionState::default().run(|| {
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
                    &mut screen(),
                    &mut MissingImages,
                    NativeSceneSoundDriver::Absent,
                )
                .unwrap();
            let version = arena.global_sector_version();
            let mut duplicate_driver = DriverSceneOwner::default();
            let duplicate = duplicate_driver
                .create_scene(
                    &mut DriverSceneContext {
                        arena: &mut arena,
                        resources: &mut resources,
                        textures: &mut textures,
                        storage: &mut storage,
                    },
                    &mut screen(),
                    &mut MissingImages,
                    NativeSceneSoundDriver::Absent,
                )
                .unwrap_err();
            assert!(matches!(
                duplicate.error,
                DriverSceneError::Resources(SceneResourceError::UnsupportedDomain(_))
            ));
            assert_eq!(arena.global_sector_version(), version);
            let mut foreign = SceneResources::default();
            let failure = driver
                .create_scene(
                    &mut DriverSceneContext {
                        arena: &mut arena,
                        resources: &mut foreign,
                        textures: &mut textures,
                        storage: &mut storage,
                    },
                    &mut screen(),
                    &mut MissingImages,
                    NativeSceneSoundDriver::Absent,
                )
                .unwrap_err();
            assert!(matches!(failure.error, DriverSceneError::ForeignDomain));
            assert_eq!(failure.scene, None);
            assert_eq!(arena.global_sector_version(), version);
            driver
                .release_scene(
                    &mut DriverSceneContext {
                        arena: &mut arena,
                        resources: &mut resources,
                        textures: &mut textures,
                        storage: &mut storage,
                    },
                    scene,
                )
                .unwrap();
        });
    }
}
