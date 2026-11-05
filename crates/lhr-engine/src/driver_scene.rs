//! Driver scene registry plus the CreateFrame dispatch. `create_scene` below
//! is a host adapter with an explicit audio dependency (it errors where audio
//! is unavailable); the overlay-credited 0x193e0 port is `driver_create_frame`,
//! which follows the native switch (listener skipped when not ready, scene
//! still pushed). Original I3D_driver::CreateFrame (PC 0x193e0) dispatch.
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
    Sound(crate::frame_sound::SoundError),
    Constructor(SceneConstructionError<E>),
    Screen(E),
    Resources(SceneResourceError),
    ForeignDomain,
    AllocationIncomplete,
    AllocationUnavailable,
    NativeSoundDriverUnavailable,
    RegistryNotEmpty,
    UnknownScene(SceneKey),
    RegisteredSceneRequiresDriverRelease(SceneKey),
}
#[derive(Debug)]
pub struct DriverSceneCreateFailure<E> {
    pub scene: Option<SceneKey>,
    pub phase: DriverSceneCreatePhase,
    pub error: DriverSceneError<E>,
}
#[derive(Debug)]
pub struct DriverSoundCreateFailure {
    pub frame: Option<crate::frame_sound::SoundKey>,
    pub error: DriverSceneError<std::convert::Infallible>,
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
    host_sound_backend: Option<crate::frame_sound::HostSoundBackend>,
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
            host_sound_backend: None,
        }
    }
}
impl DriverSceneOwner {
    /// Bind a real host PCM source/voice owner before creating the scene.
    /// This is explicit initialization of the Rust backend, not a native
    /// DirectSound driver pointer or an original-runtime validation claim.
    pub fn bind_host_sound_backend(
        &mut self,
        arena: &mut ModelArena,
        audio: &crate::runtime_audio::RuntimeAudio,
    ) -> Result<(), DriverSceneError<std::convert::Infallible>> {
        if !self.scenes.is_empty() {
            return Err(DriverSceneError::ForeignDomain);
        }
        if let Some(backend) = &self.host_sound_backend {
            if !backend.uses_audio(audio) {
                return Err(DriverSceneError::Sound(
                    crate::frame_sound::SoundError::ForeignDriver,
                ));
            }
            return arena
                .bind_host_sound_backend(backend.clone())
                .map_err(DriverSceneError::Model);
        }
        let backend = crate::frame_sound::HostSoundBackend::new(self.identity, audio);
        arena
            .bind_host_sound_backend(backend.clone())
            .map_err(DriverSceneError::Model)?;
        self.host_sound_backend = Some(backend);
        Ok(())
    }
    pub fn host_sound_backend(&self) -> Option<&crate::frame_sound::HostSoundBackend> {
        self.host_sound_backend.as_ref()
    }
    /// Actual driver owner identity for host lifetime-domain checks.
    pub(crate) fn identity(&self) -> u64 {
        self.identity
    }
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
    /// Verify the actual registry and shared frame/resource domain without
    /// constructing another owner. Camera's CreateFrame arm needs no textures.
    pub fn validate_scene_domain(
        &self,
        arena: &ModelArena,
        resources: &SceneResources,
        scene: SceneKey,
    ) -> Result<(), DriverSceneError<std::convert::Infallible>> {
        let Some(domain) = self.domains else {
            return Err(DriverSceneError::ForeignDomain);
        };
        if domain.arena != arena.lookup_reference(crate::frame_transform::FrameKey(0))
            || domain.resources != resources.identity()
        {
            return Err(DriverSceneError::ForeignDomain);
        }
        if self.allocation_incomplete {
            return Err(DriverSceneError::AllocationIncomplete);
        }
        if !self.scenes.contains(&scene) {
            return Err(DriverSceneError::UnknownScene(scene));
        }
        arena.scene(scene).map_err(DriverSceneError::Model)?;
        Ok(())
    }
    /// Original CreateFrame type3 arm: constructor reference is returned
    /// directly. Cameras are not appended to the driver's Scene registry.
    pub fn create_camera_frame<S: SceneScreen>(
        &mut self,
        arena: &mut ModelArena,
        resources: &SceneResources,
        scene: SceneKey,
        screen: &mut S,
    ) -> Result<crate::frame_camera::CameraKey, crate::frame_camera::CameraCreateFailure<S::Error>>
    {
        self.validate_scene_domain(arena, resources, scene)
            .map_err(|_| crate::frame_camera::CameraCreateFailure {
                camera: None,
                error: crate::frame_camera::CameraCreateError::DriverContext,
            })?;
        arena.create_camera(screen)
    }
    /// Original CreateFrame type12: direct constructor reference, no registry
    /// append or additional AddRef in this arm.
    /// CPU world-data composition with real Scene ownership. This is not native
    /// SceneOpen and never substitutes a no-op loadingUI callback.
    pub fn load_scene_world_cpu(
        &mut self,
        arena: &mut ModelArena,
        resources: &SceneResources,
        scene: SceneKey,
        installation: &lhr_assets::Installation,
        name: &str,
    ) -> Result<
        crate::model_loading::CpuSceneWorldProgress,
        crate::model_loading::CpuSceneWorldFailure,
    > {
        if let Err(error) = self.validate_scene_domain(arena, resources, scene) {
            return Err(crate::model_loading::CpuSceneWorldFailure {
                error: crate::model_loading::CpuSceneWorldError::DriverDomain(format!("{error:?}")),
                progress: crate::model_loading::CpuSceneWorldProgress::empty(arena, scene),
            });
        }
        arena.load_scene_world_cpu(scene, installation, name)
    }
    pub fn discard_scene_world_staging_cpu(
        &mut self,
        arena: &mut ModelArena,
        resources: &SceneResources,
        progress: &mut crate::model_loading::CpuSceneWorldProgress,
    ) -> Result<(), DriverSceneError<std::convert::Infallible>> {
        self.validate_scene_domain(arena, resources, progress.scene())?;
        arena
            .discard_scene_world_staging_cpu(progress)
            .map_err(DriverSceneError::Model)
    }
    /// Original driver type5: constructor-owned reference1, without SceneAddFrame.
    pub fn create_sector_frame(
        &mut self,
        arena: &mut ModelArena,
        resources: &SceneResources,
        scene: SceneKey,
    ) -> Result<crate::scene_ownership::SectorKey, DriverSceneError<std::convert::Infallible>> {
        self.validate_scene_domain(arena, resources, scene)?;
        arena.create_sector_frame().map_err(DriverSceneError::Model)
    }
    pub fn create_occluder_frame(
        &mut self,
        arena: &mut ModelArena,
        resources: &SceneResources,
        scene: SceneKey,
    ) -> Result<crate::frame_occluder::OccluderKey, DriverSceneError<std::convert::Infallible>>
    {
        self.validate_scene_domain(arena, resources, scene)?;
        arena.create_occluder().map_err(DriverSceneError::Model)
    }
    /// Original CreateFrame type6 arm: one actual I3D_dummy constructor
    /// reference, without sound/scene registry insertion or an extra AddRef.
    pub fn create_area_frame(
        &mut self,
        arena: &mut ModelArena,
        resources: &SceneResources,
        scene: SceneKey,
    ) -> Result<crate::frame_area::AreaKey, DriverSceneError<std::convert::Infallible>> {
        self.validate_scene_domain(arena, resources, scene)?;
        arena.create_area().map_err(DriverSceneError::Model)
    }
    pub fn create_dummy_frame(
        &mut self,
        arena: &mut ModelArena,
        resources: &SceneResources,
        scene: SceneKey,
    ) -> Result<crate::frame_transform::FrameKey, DriverSceneError<std::convert::Infallible>> {
        self.validate_scene_domain(arena, resources, scene)?;
        arena.create_dummy_frame().map_err(DriverSceneError::Model)
    }
    /// Original type2 arm delegates the existing canonical light constructor
    /// and CRT stream; lights have no scene registry append here.
    pub fn create_light_frame(
        &mut self,
        arena: &mut ModelArena,
        resources: &SceneResources,
        scene: SceneKey,
    ) -> Result<crate::frame_transform::FrameKey, DriverSceneError<std::convert::Infallible>> {
        self.validate_scene_domain(arena, resources, scene)?;
        arena.create_light().map_err(DriverSceneError::Model)
    }
    /// Original type9 direct model constructor arm. Scene membership remains
    /// a distinct caller operation; the model's creator reference is returned.
    pub fn create_model_frame(
        &mut self,
        arena: &mut ModelArena,
        resources: &SceneResources,
        scene: SceneKey,
    ) -> Result<crate::model_loading::ModelKey, DriverSceneError<std::convert::Infallible>> {
        self.validate_scene_domain(arena, resources, scene)?;
        arena.create_model().map_err(DriverSceneError::Model)
    }
    /// Native CreateFrame case 4: create a real sound frame with a derived
    /// host PCM owner and append it to this driver's borrowed sound registry.
    pub fn create_sound_frame(
        &mut self,
        arena: &mut ModelArena,
        resources: &SceneResources,
        scene: SceneKey,
    ) -> Result<crate::frame_sound::SoundKey, DriverSoundCreateFailure> {
        self.validate_scene_domain(arena, resources, scene)
            .map_err(|error| DriverSoundCreateFailure { frame: None, error })?;
        arena
            .create_owned_sound_frame()
            .map_err(|failure| DriverSoundCreateFailure {
                frame: failure.frame,
                error: DriverSceneError::Model(failure.error),
            })
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
        match sound {
            NativeSceneSoundDriver::Absent => {}
            NativeSceneSoundDriver::Uninitialized => {
                return Err(failure(
                    Some(scene),
                    DriverSceneCreatePhase::Listener,
                    DriverSceneError::NativeSoundDriverUnavailable,
                ));
            }
            NativeSceneSoundDriver::Initialized => {
                self.host_sound_backend
                    .as_ref()
                    .ok_or_else(|| {
                        failure(
                            Some(scene),
                            DriverSceneCreatePhase::Listener,
                            DriverSceneError::NativeSoundDriverUnavailable,
                        )
                    })?
                    .create_listener(scene)
                    .map_err(|error| {
                        failure(
                            Some(scene),
                            DriverSceneCreatePhase::Listener,
                            DriverSceneError::Sound(error),
                        )
                    })?;
            }
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
        let backend = self.host_sound_backend.clone();
        let mut release_listener = move |scene| {
            if let Some(backend) = &backend {
                if backend
                    .listeners()
                    .map_err(SceneResourceError::Sound)?
                    .contains(&scene)
                {
                    backend
                        .release_listener(scene)
                        .map_err(SceneResourceError::Sound)?;
                }
            }
            Ok(())
        };
        context
            .resources
            .release_driver_scene(
                context.arena,
                scene,
                context.textures,
                context.storage,
                crate::scene_resources::DriverSceneRemoval {
                    driver: identity,
                    release_listener: &mut release_listener,
                    unregister: &mut |scene| {
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
                },
            )
            .map_err(DriverSceneError::Resources)?;
        Ok(())
    }

    /// Host terminal cleanup for a Scene whose factory reached CreateListener
    /// but failed before driver registration. The caller first releases the
    /// unregistered Scene through SceneResources; registered Scenes must use
    /// release_scene so the listener follows the same last-release boundary.
    pub fn release_unregistered_scene_listener(
        &mut self,
        scene: SceneKey,
    ) -> Result<(), DriverSceneError<std::convert::Infallible>> {
        if self.scenes.contains(&scene) {
            return Err(DriverSceneError::RegisteredSceneRequiresDriverRelease(
                scene,
            ));
        }
        if let Some(backend) = &self.host_sound_backend {
            if backend
                .listeners()
                .map_err(DriverSceneError::Sound)?
                .contains(&scene)
            {
                backend
                    .release_listener(scene)
                    .map_err(DriverSceneError::Sound)?;
            }
        }
        Ok(())
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
    fn initialized_listener_survives_failed_registration_until_explicit_cleanup() {
        Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let mut resources = SceneResources::default();
            let mut textures = TextureResources::new().unwrap();
            let mut storage = CpuTextureStorage::default();
            let mut driver = DriverSceneOwner::default();
            let audio = crate::runtime_audio::RuntimeAudio::default();
            driver.bind_host_sound_backend(&mut arena, &audio).unwrap();
            let mut context = DriverSceneContext {
                arena: &mut arena,
                resources: &mut resources,
                textures: &mut textures,
                storage: &mut storage,
            };
            let failure = driver
                .create_scene_with_allocator(
                    &mut context,
                    &mut screen(),
                    &mut MissingImages,
                    NativeSceneSoundDriver::Initialized,
                    |capacity, scenes| {
                        assert_eq!(capacity, 32);
                        assert!(scenes.is_empty());
                        Err(())
                    },
                )
                .unwrap_err();
            assert_eq!(failure.phase, DriverSceneCreatePhase::Registration);
            assert!(matches!(
                failure.error,
                DriverSceneError::AllocationUnavailable
            ));
            let scene = failure.scene.unwrap();
            assert_eq!(
                driver.host_sound_backend().unwrap().listeners().unwrap(),
                [scene]
            );
            assert!(driver.scenes().is_empty());
            assert!(driver.allocation_incomplete());
            assert!(matches!(
                driver.release_scene(&mut context, scene),
                Err(DriverSceneError::AllocationIncomplete)
            ));
            assert_eq!(
                driver.host_sound_backend().unwrap().listeners().unwrap(),
                [scene]
            );
            context
                .resources
                .release_scene(context.arena, scene, context.textures, context.storage)
                .unwrap();
            driver.release_unregistered_scene_listener(scene).unwrap();
            assert!(driver
                .host_sound_backend()
                .unwrap()
                .listeners()
                .unwrap()
                .is_empty());
            assert!(matches!(
                driver.release_empty_registry_storage(),
                Err(DriverSceneError::AllocationIncomplete)
            ));
            assert_eq!(audio.live_sound_count().unwrap(), 0);
        });
    }
    #[test]
    fn retained_sound_survives_last_scene_and_idempotent_backend_bind() {
        Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let mut resources = SceneResources::default();
            let mut textures = TextureResources::new().unwrap();
            let mut storage = CpuTextureStorage::default();
            let mut driver = DriverSceneOwner::default();
            let audio = crate::runtime_audio::RuntimeAudio::default();
            let foreign_audio = crate::runtime_audio::RuntimeAudio::default();
            driver.bind_host_sound_backend(&mut arena, &audio).unwrap();
            let mut context = DriverSceneContext {
                arena: &mut arena,
                resources: &mut resources,
                textures: &mut textures,
                storage: &mut storage,
            };
            let scene = driver
                .create_scene(
                    &mut context,
                    &mut screen(),
                    &mut MissingImages,
                    NativeSceneSoundDriver::Initialized,
                )
                .unwrap();
            let sound = driver
                .create_sound_frame(context.arena, context.resources, scene)
                .unwrap();
            // A cache can retain its SOUND frame after the Scene that caused
            // its creation is released. Transfer the creator's reference to
            // that owner without replacing the canonical frame or registry.
            context.arena.retain(sound.frame()).unwrap();
            context.arena.release(sound.frame()).unwrap();
            driver.release_scene(&mut context, scene).unwrap();
            assert!(driver.scenes().is_empty());
            assert!(driver
                .host_sound_backend()
                .unwrap()
                .listeners()
                .unwrap()
                .is_empty());
            assert_eq!(
                driver.host_sound_backend().unwrap().sounds().unwrap(),
                [sound]
            );
            assert_eq!(audio.live_sound_count().unwrap(), 1);

            driver
                .bind_host_sound_backend(context.arena, &audio.clone())
                .unwrap();
            assert!(matches!(
                driver.bind_host_sound_backend(context.arena, &foreign_audio),
                Err(DriverSceneError::Sound(
                    crate::frame_sound::SoundError::ForeignDriver
                ))
            ));
            assert!(matches!(
                context
                    .arena
                    .bind_host_sound_backend(crate::frame_sound::HostSoundBackend::new(
                        driver.identity,
                        &audio
                    )),
                Err(ModelError::Sound(
                    crate::frame_sound::SoundError::ForeignDriver
                ))
            ));
            assert_eq!(
                driver.host_sound_backend().unwrap().sounds().unwrap(),
                [sound]
            );
            context.arena.release(sound.frame()).unwrap();
            assert!(driver
                .host_sound_backend()
                .unwrap()
                .sounds()
                .unwrap()
                .is_empty());
            assert_eq!(audio.live_sound_count().unwrap(), 0);
            assert_eq!(foreign_audio.live_sound_count().unwrap(), 0);
        });
    }
    #[test]
    fn initialized_backend_retains_each_listener_until_its_scene_last_release() {
        Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let mut resources = SceneResources::default();
            let mut textures = TextureResources::new().unwrap();
            let mut storage = CpuTextureStorage::default();
            let mut driver = DriverSceneOwner::default();
            let audio = crate::runtime_audio::RuntimeAudio::default();
            driver.bind_host_sound_backend(&mut arena, &audio).unwrap();
            let mut context = DriverSceneContext {
                arena: &mut arena,
                resources: &mut resources,
                textures: &mut textures,
                storage: &mut storage,
            };
            let first = driver
                .create_scene(
                    &mut context,
                    &mut screen(),
                    &mut MissingImages,
                    NativeSceneSoundDriver::Initialized,
                )
                .unwrap();
            let second = driver
                .create_scene(
                    &mut context,
                    &mut screen(),
                    &mut MissingImages,
                    NativeSceneSoundDriver::Initialized,
                )
                .unwrap();
            assert_eq!(
                driver.host_sound_backend().unwrap().listeners().unwrap(),
                [first, second]
            );
            context.arena.retain(first.root()).unwrap();
            driver.release_scene(&mut context, first).unwrap();
            assert!(
                !context
                    .resources
                    .construction(first)
                    .unwrap()
                    .listener_released
            );
            assert_eq!(context.resources.ws_ref_count(), 2);
            assert_eq!(
                driver.host_sound_backend().unwrap().listeners().unwrap(),
                [first, second]
            );
            driver.release_scene(&mut context, second).unwrap();
            assert!(
                context
                    .resources
                    .construction(second)
                    .unwrap()
                    .listener_released
            );
            assert_eq!(context.resources.ws_ref_count(), 1);
            assert_eq!(
                driver.host_sound_backend().unwrap().listeners().unwrap(),
                [first]
            );
            driver.release_scene(&mut context, first).unwrap();
            assert!(
                context
                    .resources
                    .construction(first)
                    .unwrap()
                    .listener_released
            );
            assert!(driver
                .host_sound_backend()
                .unwrap()
                .listeners()
                .unwrap()
                .is_empty());
            assert_eq!(context.resources.ws_ref_count(), 0);
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

// I3D_driver::CreateFrame full 16-arm dispatch (LS3DF RVA 0x193e0).
// Upstream reference mafia-re ls3df/I3D/I3d_driver.cpp @ ab67b13:1433-1502,
// driver lists I3D.h:4403-4415 (sounds at +0x38, scenes at +0x48), frame enum
// I3D.h:62-83. Name CreateFrame is an evidenced PS2 original (I3D_driver
// vtable); Rust spelling is snake_case per repo rule. Tier contract
// (new-module wave policy: contract regardless of bodies; LAB_ entry shape per
// assignment). The operator-new bodies stay behind the allocator backend; the
// viewport rectangle, listener readiness and invalid-type diagnostic stay
// behind backend traits. Existing DriverSceneOwner arms above keep the full
// scene/sound constructor paths; this dispatch pins the switch shape, the two
// list pushes and the NULL/FATAL edges for all sixteen arms.

/// Frame words dispatched by CreateFrame (I3D.h:62-83). FRAME_NULL (0),
/// FRAME_VISUAL (1) and FRAME_LAST+ fall through to the invalid arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FactoryFrameKind {
    Light,
    Camera,
    Sound,
    Sector,
    Dummy,
    Target,
    User,
    Model,
    Joint,
    Volume,
    Occluder,
    Scene,
    Area,
    Shadow,
    Landscape,
    Emitor,
}

impl FactoryFrameKind {
    /// Native I3D_FRAME_TYPE word for this arm.
    pub fn word(self) -> u32 {
        match self {
            Self::Light => 2,
            Self::Camera => 3,
            Self::Sound => 4,
            Self::Sector => 5,
            Self::Dummy => 6,
            Self::Target => 7,
            Self::User => 8,
            Self::Model => 9,
            Self::Joint => 10,
            Self::Volume => 11,
            Self::Occluder => 12,
            Self::Scene => 13,
            Self::Area => 14,
            Self::Shadow => 15,
            Self::Landscape => 16,
            Self::Emitor => 17,
        }
    }

    /// Sixteen-arm classification. Words 0, 1 and 18+ report no kind.
    pub fn from_word(word: u32) -> Option<Self> {
        match word {
            2 => Some(Self::Light),
            3 => Some(Self::Camera),
            4 => Some(Self::Sound),
            5 => Some(Self::Sector),
            6 => Some(Self::Dummy),
            7 => Some(Self::Target),
            8 => Some(Self::User),
            9 => Some(Self::Model),
            10 => Some(Self::Joint),
            11 => Some(Self::Volume),
            12 => Some(Self::Occluder),
            13 => Some(Self::Scene),
            14 => Some(Self::Area),
            15 => Some(Self::Shadow),
            16 => Some(Self::Landscape),
            17 => Some(Self::Emitor),
            _ => None,
        }
    }
}

/// Owned frame identity produced by the operator-new boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DriverFrameHandle {
    kind: FactoryFrameKind,
    serial: u64,
}

impl DriverFrameHandle {
    pub fn fresh(kind: FactoryFrameKind) -> Self {
        static NEXT_FACTORY_FRAME: AtomicU64 = AtomicU64::new(1);
        let serial = NEXT_FACTORY_FRAME
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .expect("driver frame identity exhausted");
        Self { kind, serial }
    }

    pub fn kind(self) -> FactoryFrameKind {
        self.kind
    }

    pub fn serial(self) -> u64 {
        self.serial
    }
}

/// Typed image of the driver lists mutated by CreateFrame: sounds (+0x38) and
/// scenes (+0x48). Entries are borrowed identities; removal happens in the
/// frame destructors, never here.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DriverFrameRegistry {
    pub sounds: Vec<DriverFrameHandle>,
    pub scenes: Vec<DriverFrameHandle>,
}

/// Operator-new plus scene-arm host boundaries. Allocation covers the per-arm
/// new; the viewport pair covers GetViewPort/SetViewport; the listener pair
/// covers the `pISndDriver && IsInit()` gate with CreateListener.
pub trait DriverFrameFactoryBackend {
    fn allocate_frame(&mut self, kind: FactoryFrameKind) -> Option<DriverFrameHandle>;
    fn out_of_memory_fatal(&mut self);
    /// Native GetViewPort fields [x, y, width, height].
    fn viewport_rectangle(&mut self) -> [i32; 4];
    fn set_viewport(&mut self, scene: DriverFrameHandle, rect: [i32; 4]);
    fn sound_listener_ready(&self) -> bool;
    fn create_listener(&mut self, scene: DriverFrameHandle);
}

/// Diagnostic sink for the invalid frame-type message.
pub trait DriverFrameDiagnostic {
    fn invalid_frame_type(&mut self, type_word: u32);
}

/// Factory-path registries (DriverFrameRegistry.sounds/scenes) serve this
/// dispatch; the pre-existing DriverSceneOwner registries serve the arena
/// path. The models cover disjoint native lists; no cross-model consistency
/// is claimed at contract tier.
/// LS3DF RVA 0x193e0. Evidenced name I3D_driver::CreateFrame from mafia-re
/// I3d_driver.cpp:1433-1502. Sixteen arms dispatched by type word (table-driven;
// enum declaration order is host-side, behavior-neutral); FRAME_SOUND pushes
/// the new handle onto sounds; FRAME_SCENE sets the viewport with wrapping
/// width/height subtraction, creates the listener when the sound driver is
/// present and initialized, then pushes onto scenes; any other known arm
/// returns the handle with no list effect; an invalid word emits the
/// diagnostic and returns NULL; a failed allocation invokes the fatal handler
/// and returns NULL with no list push. SCENE OOM hardening: native
/// allocates then dereferences for SetViewport before any null check (a
/// fault); the host reports fatal and returns NULL instead (pinned by
/// oom_invokes_fatal_with_no_list_push as adaptation coverage, and the
/// entry stays contract on that edge).
pub fn driver_create_frame(
    registry: &mut DriverFrameRegistry,
    type_word: u32,
    backend: &mut impl DriverFrameFactoryBackend,
    diag: &mut impl DriverFrameDiagnostic,
) -> Option<DriverFrameHandle> {
    let Some(kind) = FactoryFrameKind::from_word(type_word) else {
        diag.invalid_frame_type(type_word);
        return None;
    };
    match kind {
        FactoryFrameKind::Sound => {
            let Some(handle) = backend.allocate_frame(kind) else {
                backend.out_of_memory_fatal();
                return None;
            };
            registry.sounds.push(handle);
            Some(handle)
        }
        FactoryFrameKind::Scene => {
            let Some(handle) = backend.allocate_frame(kind) else {
                backend.out_of_memory_fatal();
                return None;
            };
            let viewport = backend.viewport_rectangle();
            backend.set_viewport(
                handle,
                [
                    viewport[0],
                    viewport[1],
                    viewport[2].wrapping_sub(viewport[0]),
                    viewport[3].wrapping_sub(viewport[1]),
                ],
            );
            if backend.sound_listener_ready() {
                backend.create_listener(handle);
            }
            registry.scenes.push(handle);
            Some(handle)
        }
        _ => {
            let Some(handle) = backend.allocate_frame(kind) else {
                backend.out_of_memory_fatal();
                return None;
            };
            Some(handle)
        }
    }
}

#[cfg(test)]
mod factory_dispatch_tests {
    use super::*;

    struct TestBackend {
        fail_on: Option<FactoryFrameKind>,
        fatal: usize,
        viewport: [i32; 4],
        viewports_set: Vec<(DriverFrameHandle, [i32; 4])>,
        listener_ready: bool,
        listeners: Vec<DriverFrameHandle>,
        allocated: Vec<FactoryFrameKind>,
    }

    impl TestBackend {
        fn ok() -> Self {
            Self {
                fail_on: None,
                fatal: 0,
                viewport: [10, 20, 810, 620],
                viewports_set: Vec::new(),
                listener_ready: false,
                listeners: Vec::new(),
                allocated: Vec::new(),
            }
        }
    }

    impl DriverFrameFactoryBackend for TestBackend {
        fn allocate_frame(&mut self, kind: FactoryFrameKind) -> Option<DriverFrameHandle> {
            self.allocated.push(kind);
            if self.fail_on == Some(kind) {
                return None;
            }
            Some(DriverFrameHandle::fresh(kind))
        }

        fn out_of_memory_fatal(&mut self) {
            self.fatal += 1;
        }

        fn viewport_rectangle(&mut self) -> [i32; 4] {
            self.viewport
        }

        fn set_viewport(&mut self, scene: DriverFrameHandle, rect: [i32; 4]) {
            self.viewports_set.push((scene, rect));
        }

        fn sound_listener_ready(&self) -> bool {
            self.listener_ready
        }

        fn create_listener(&mut self, scene: DriverFrameHandle) {
            self.listeners.push(scene);
        }
    }

    #[derive(Default)]
    struct TestDiag {
        invalid: Vec<u32>,
    }

    impl DriverFrameDiagnostic for TestDiag {
        fn invalid_frame_type(&mut self, type_word: u32) {
            self.invalid.push(type_word);
        }
    }

    #[test]
    fn sixteen_arms_classify_and_plain_arms_push_no_list() {
        let mut registry = DriverFrameRegistry::default();
        let mut backend = TestBackend::ok();
        let mut diag = TestDiag::default();
        let plain = [2, 3, 5, 6, 7, 8, 9, 10, 11, 12, 14, 15, 16, 17];
        for word in plain {
            let handle = driver_create_frame(&mut registry, word, &mut backend, &mut diag)
                .expect("plain arm constructs");
            assert_eq!(handle.kind(), FactoryFrameKind::from_word(word).unwrap());
            assert!(handle.word_mirrors(word));
        }
        assert!(registry.sounds.is_empty());
        assert!(registry.scenes.is_empty());
        assert!(diag.invalid.is_empty());
        assert_eq!(backend.fatal, 0);
        assert!(backend.viewports_set.is_empty());
        assert!(backend.listeners.is_empty());
        assert_eq!(backend.allocated.len(), 14);
    }

    #[test]
    fn sound_arm_pushes_sounds_and_scene_arm_does_viewport_listener_push() {
        let mut registry = DriverFrameRegistry::default();
        let mut backend = TestBackend::ok();
        let mut diag = TestDiag::default();
        let sound = driver_create_frame(&mut registry, 4, &mut backend, &mut diag).unwrap();
        assert_eq!(sound.kind(), FactoryFrameKind::Sound);
        assert_eq!(registry.sounds, vec![sound]);
        assert!(registry.scenes.is_empty());
        backend.listener_ready = true;
        let scene = driver_create_frame(&mut registry, 13, &mut backend, &mut diag).unwrap();
        assert_eq!(scene.kind(), FactoryFrameKind::Scene);
        assert_eq!(registry.scenes, vec![scene]);
        assert_eq!(registry.sounds, vec![sound]);
        assert_eq!(backend.viewports_set, vec![(scene, [10, 20, 800, 600])]);
        assert_eq!(backend.listeners, vec![scene]);
        let mut bare = TestBackend::ok();
        bare.viewport = [5, 7, 3, 9];
        let mut registry2 = DriverFrameRegistry::default();
        let mut diag2 = TestDiag::default();
        let scene2 = driver_create_frame(&mut registry2, 13, &mut bare, &mut diag2).unwrap();
        assert_eq!(bare.viewports_set, vec![(scene2, [5, 7, -2, 2])]);
        assert!(bare.listeners.is_empty());
    }

    #[test]
    fn invalid_word_reports_and_skips_allocation() {
        let mut registry = DriverFrameRegistry::default();
        let mut backend = TestBackend::ok();
        let mut diag = TestDiag::default();
        for word in [0u32, 1, 18, 19, 0xffff_ffff] {
            assert_eq!(
                driver_create_frame(&mut registry, word, &mut backend, &mut diag),
                None
            );
        }
        assert_eq!(diag.invalid, vec![0, 1, 18, 19, 0xffff_ffff]);
        assert!(backend.allocated.is_empty());
        assert!(registry.sounds.is_empty() && registry.scenes.is_empty());
    }

    #[test]
    fn oom_invokes_fatal_with_no_list_push() {
        let mut registry = DriverFrameRegistry::default();
        let mut backend = TestBackend::ok();
        backend.fail_on = Some(FactoryFrameKind::Sound);
        let mut diag = TestDiag::default();
        assert_eq!(
            driver_create_frame(&mut registry, 4, &mut backend, &mut diag),
            None
        );
        assert_eq!(backend.fatal, 1);
        assert!(registry.sounds.is_empty());
        backend.fail_on = Some(FactoryFrameKind::Scene);
        assert_eq!(
            driver_create_frame(&mut registry, 13, &mut backend, &mut diag),
            None
        );
        assert_eq!(backend.fatal, 2);
        assert!(registry.scenes.is_empty());
        assert!(backend.viewports_set.is_empty() && backend.listeners.is_empty());
        assert!(diag.invalid.is_empty());
    }

    impl DriverFrameHandle {
        fn word_mirrors(self, word: u32) -> bool {
            self.kind().word() == word
        }
    }
}
