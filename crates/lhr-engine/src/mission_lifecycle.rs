//! C_mission lifetime, Game.exe SHA-256
//! 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27.
//! Constructor RVA 0x13f010; Close RVA 0x1405e0, unopened branch only.
//! Original PS2 C_mission names corroborate identity, not PC member offsets.
//! Explicit seeded storage preserves uninitialized pointers, padding and far range.
//! This owner does not open missions or initialize the game.

use crate::c_str::CStrBackend;
use crate::driver_scene::{
    DriverSceneContext, DriverSceneOwner, DriverSceneScreen, NativeSceneSoundDriver,
};
use crate::itexture::TextureImageSource;
use crate::mission_components::{
    GameConstructorGlobals, GameConstructorInputs, MissionAllocation, MissionAllocator,
    MissionComponent, MissionComponentKind,
};
use crate::mission_resources::MissionCaches;
use crate::scene_ownership::SceneKey;

#[derive(Clone, Copy, Debug)]
pub struct MissionInitInputs {
    /// Observer identity bound exclusively to the actual returned SceneKey.
    /// It is never dereferenced or used to create a second frame owner.
    pub scene_identity: u32,
    pub effects_stack_control: u8,
    pub game: GameConstructorInputs,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MissionPhase {
    CreateScene,
    Allocate(MissionComponentKind),
    Construct(MissionComponentKind),
    ClearCaches,
    Close,
    Destroy(MissionComponentKind),
    Free(MissionComponentKind),
    ReleaseScene,
    FreeUnknown68,
    DestroyVector(usize),
    Complete,
}
#[derive(Debug)]
pub struct MissionFailure {
    pub phase: MissionPhase,
    pub detail: String,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LifecycleDomains {
    arena: crate::script_scheduler::FrameReferenceId,
    resources: u64,
    textures: u64,
    storage: u64,
    memory: crate::mission_components::MissionMemoryDomainId,
}
struct OwnedComponent {
    value: MissionComponent,
    alive: bool,
}

/// Original PC allocation size, independent of the larger PS2 mission.
pub const MISSION_BYTES: usize = 0xdc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MissionLifecycleError {
    /// Active Close requires the game, actors, programs and scene Close owners.
    ActiveCloseUnavailable,
    Destructed,
}

pub struct MissionLifecycle {
    identity: u32,
    bytes: [u8; MISSION_BYTES],
    destructed: bool,
    scene: Option<SceneKey>,
    pending_scene: Option<SceneKey>,
    components: Vec<OwnedComponent>,
    pending_component: Option<MissionComponent>,
    rejected_allocation: Option<MissionAllocation>,
    phase: MissionPhase,
    interrupted: bool,
    domains: Option<LifecycleDomains>,
}
impl MissionLifecycle {
    /// Execute the constructor's stores over caller-supplied allocation bytes.
    /// The vector control byte comes from saved ECX's most significant byte.
    pub fn construct(identity: u32, seed: [u8; MISSION_BYTES]) -> Self {
        let mut out = Self {
            identity,
            bytes: seed,
            destructed: false,
            scene: None,
            pending_scene: None,
            components: Vec::new(),
            pending_component: None,
            rejected_allocation: None,
            phase: MissionPhase::Complete,
            interrupted: false,
            domains: None,
        };
        for offset in [0, 4, 0x10, 0x28, 0x68] {
            out.put_word(offset, 0);
        }
        out.bytes[0x0c] = 0;
        for offset in [0x14, 0x34, 0x44, 0x54, 0x80, 0xac, 0xbc, 0xcc] {
            out.bytes[offset] = (identity >> 24) as u8;
            for field in [4, 8, 12] {
                out.put_word(offset + field, 0);
            }
        }
        for offset in [0x90, 0x94] {
            out.put_word(offset, 1f32.to_bits());
        }
        for offset in [0x9c, 0xa0, 0xa8] {
            out.put_word(offset, 2);
        }
        out.put_word(0xa4, 3);
        out
    }
    pub fn scene(&self) -> Option<SceneKey> {
        self.scene
    }
    pub fn pending_scene(&self) -> Option<SceneKey> {
        self.pending_scene
    }
    pub fn pending_component(&self) -> Option<&MissionComponent> {
        self.pending_component.as_ref()
    }
    pub fn components(&self) -> impl Iterator<Item = (&MissionComponent, bool)> {
        self.components
            .iter()
            .map(|entry| (&entry.value, entry.alive))
    }
    pub fn phase(&self) -> MissionPhase {
        self.phase
    }
    pub fn interrupted(&self) -> bool {
        self.interrupted
    }
    fn fail(&mut self, detail: impl std::fmt::Debug) -> MissionFailure {
        self.interrupted = true;
        MissionFailure {
            phase: self.phase,
            detail: format!("{detail:?}"),
        }
    }
    fn available(&self) -> Result<(), MissionFailure> {
        if self.destructed || self.interrupted {
            Err(MissionFailure {
                phase: self.phase,
                detail: "mission lifetime unavailable after destruction/interruption".into(),
            })
        } else {
            Ok(())
        }
    }
    fn bind<A: MissionAllocator>(
        &mut self,
        context: &DriverSceneContext<'_>,
        allocator: &A,
    ) -> Result<(), MissionFailure> {
        let domains = LifecycleDomains {
            arena: context
                .arena
                .lookup_reference(crate::frame_transform::FrameKey(0)),
            resources: context.resources.identity(),
            textures: context.textures.objects().managed().resources().identity(),
            storage: context.storage.identity(),
            memory: allocator.mission_memory_domain(),
        };
        if self.domains.is_some_and(|bound| bound != domains) {
            return Err(MissionFailure {
                phase: self.phase,
                detail: "foreign mission resource/allocator domain".into(),
            });
        }
        self.domains = Some(domains);
        Ok(())
    }
    /// Actual Init53f230 over the existing type13 SceneDriver/resources. The
    /// outer scene member is published only when its real factory returns.
    /// Unsupported work retains the executed prefix and its owned allocations.
    #[allow(clippy::too_many_arguments)]
    pub fn init<S: DriverSceneScreen, A: MissionAllocator>(
        &mut self,
        driver: &mut DriverSceneOwner,
        context: &mut DriverSceneContext<'_>,
        screen: &mut S,
        images: &mut impl TextureImageSource,
        sound: NativeSceneSoundDriver,
        allocator: &mut A,
        inputs: MissionInitInputs,
        globals: &mut GameConstructorGlobals,
    ) -> Result<(), MissionFailure>
    where
        A::Error: std::fmt::Debug,
        S::Error: std::fmt::Debug,
    {
        self.available()?;
        // Native returns without even reading the driver in this branch.
        if self.word(0x10) != 0 {
            return Ok(());
        }
        self.bind(context, allocator)?;
        self.phase = MissionPhase::CreateScene;
        if inputs.scene_identity == 0 {
            return Err(self.fail("null observer identity cannot represent the returned Scene"));
        }
        let scene = match driver.create_scene(context, screen, images, sound) {
            Ok(scene) => scene,
            Err(error) => {
                self.pending_scene = error.scene;
                return Err(self.fail(error));
            }
        };
        self.scene = Some(scene);
        self.put_word(0x10, inputs.scene_identity);
        for (kind, offset) in [
            (MissionComponentKind::Tshift, 0x28),
            (MissionComponentKind::WebPath, 0x2c),
            (MissionComponentKind::Roads, 0x30),
            (MissionComponentKind::Effects, 0x6c),
        ] {
            if self.word(offset) != 0 {
                continue;
            }
            self.phase = MissionPhase::Allocate(kind);
            let allocation = allocator.allocate(kind).map_err(|e| self.fail(e))?;
            let Some(allocation) = allocation else {
                self.put_word(offset, 0);
                continue;
            };
            if allocation.domain != allocator.mission_memory_domain()
                || allocation.identity == 0
                || allocation.bytes.len() != kind.allocation_size()
            {
                self.rejected_allocation = Some(allocation);
                return Err(self.fail("invalid caller allocation"));
            }
            self.phase = MissionPhase::Construct(kind);
            let mut component = if kind == MissionComponentKind::Effects {
                MissionComponent::construct_effects_prefix(allocation, inputs.effects_stack_control)
                    .expect("validated allocation")
            } else {
                MissionComponent::construct(kind, allocation)
                    .expect("validated admitted constructor")
            };
            if kind == MissionComponentKind::Effects {
                if let Err(error) = component.finish_effects_constructor(allocator) {
                    self.pending_component = Some(component);
                    return Err(self.fail(error));
                }
            }
            self.put_word(offset, component.identity());
            self.components.push(OwnedComponent {
                value: component,
                alive: true,
            });
        }
        self.put_word(0x88, self.word(0x84));
        if self.word(0x24) == 0 {
            let kind = MissionComponentKind::Game;
            self.phase = MissionPhase::Allocate(kind);
            let allocation = allocator.allocate(kind).map_err(|e| self.fail(e))?;
            let Some(allocation) = allocation else {
                self.put_word(0x24, 0);
                return Err(self.fail("original null Game write at 0x53f38f to address0x44"));
            };
            if allocation.domain != allocator.mission_memory_domain()
                || allocation.identity == 0
                || allocation.bytes.len() != kind.allocation_size()
            {
                self.rejected_allocation = Some(allocation);
                return Err(self.fail("invalid caller Game allocation"));
            }
            self.phase = MissionPhase::Construct(kind);
            let mut component = MissionComponent::construct_game(allocation, inputs.game, globals)
                .expect("validated allocation");
            self.put_word(0x24, component.identity());
            component.put(0x44, self.identity);
            self.components.push(OwnedComponent {
                value: component,
                alive: true,
            });
        }
        self.phase = MissionPhase::Complete;
        Ok(())
    }
    /// Exact UnInit order over the same common caches and genuine scene owner.
    /// Only unopened Close and constructor-empty support members are admitted.
    pub fn uninit<A>(
        &mut self,
        caches: &mut MissionCaches,
        driver: &mut DriverSceneOwner,
        context: &mut DriverSceneContext<'_>,
        allocator: &mut A,
    ) -> Result<(), MissionFailure>
    where
        A: MissionAllocator + CStrBackend<Error = <A as MissionAllocator>::Error>,
        <A as MissionAllocator>::Error: std::fmt::Debug,
    {
        self.available()?;
        self.bind(context, allocator)?;
        if let Some(scene) = self.scene {
            driver
                .validate_scene_domain(context.arena, context.resources, scene)
                .map_err(|e| MissionFailure {
                    phase: self.phase,
                    detail: format!("{e:?}"),
                })?;
        }
        self.phase = MissionPhase::ClearCaches;
        caches
            .clear_for_mission(context.arena, allocator)
            .map_err(|e| self.fail(e))?;
        self.phase = MissionPhase::Close;
        self.close().map_err(|e| self.fail(e))?;
        for (kind, offset) in [
            (MissionComponentKind::Tshift, 0x28),
            (MissionComponentKind::WebPath, 0x2c),
            (MissionComponentKind::Roads, 0x30),
            (MissionComponentKind::Effects, 0x6c),
            (MissionComponentKind::Game, 0x24),
        ] {
            let identity = self.word(offset);
            if identity == 0 {
                continue;
            }
            self.phase = MissionPhase::Destroy(kind);
            let Some(index) = self.components.iter().position(|entry| {
                entry.alive && entry.value.identity() == identity && entry.value.kind() == kind
            }) else {
                return Err(self.fail("component pointer has no matching owned allocation"));
            };
            self.components[index]
                .value
                .destroy(allocator)
                .map_err(|e| self.fail(e))?;
            self.phase = MissionPhase::Free(kind);
            allocator.free(identity).map_err(|e| self.fail(e))?;
            self.components[index].alive = false;
            self.put_word(offset, 0);
        }
        if self.word(0x10) != 0 {
            self.phase = MissionPhase::Close;
            self.close().map_err(|e| self.fail(e))?;
            self.phase = MissionPhase::ReleaseScene;
            let scene = self
                .scene
                .ok_or_else(|| self.fail("scene pointer has no matching owned SceneKey"))?;
            driver
                .release_scene(context, scene)
                .map_err(|e| self.fail(e))?;
            self.scene = None;
            self.put_word(0x10, 0);
        }
        self.phase = MissionPhase::FreeUnknown68;
        if self.word(0x68) != 0 {
            return Err(self.fail("unknown68 allocation owner unavailable"));
        }
        self.phase = MissionPhase::Complete;
        Ok(())
    }
    pub fn destruct<A>(
        &mut self,
        caches: &mut MissionCaches,
        driver: &mut DriverSceneOwner,
        context: &mut DriverSceneContext<'_>,
        allocator: &mut A,
    ) -> Result<(), MissionFailure>
    where
        A: MissionAllocator + CStrBackend<Error = <A as MissionAllocator>::Error>,
        <A as MissionAllocator>::Error: std::fmt::Debug,
    {
        self.uninit(caches, driver, context, allocator)?;
        for offset in [0xcc, 0xbc, 0xac, 0x80, 0x54, 0x44, 0x34, 0x14] {
            self.phase = MissionPhase::DestroyVector(offset);
            if self.word(offset + 4) != 0 {
                return Err(self.fail("populated mission vector allocation owner unavailable"));
            }
            allocator.free(0).map_err(|e| self.fail(e))?;
            for n in [4, 8, 12] {
                self.put_word(offset + n, 0);
            }
        }
        self.destructed = true;
        self.phase = MissionPhase::Complete;
        Ok(())
    }
    pub fn identity(&self) -> u32 {
        self.identity
    }
    pub fn raw_bytes(&self) -> &[u8; MISSION_BYTES] {
        &self.bytes
    }
    pub fn is_destructed(&self) -> bool {
        self.destructed
    }
    fn word(&self, offset: usize) -> u32 {
        u32::from_le_bytes(self.bytes[offset..offset + 4].try_into().unwrap())
    }
    fn put_word(&mut self, offset: usize, value: u32) {
        self.bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    /// The actual original early-return guard, with no fabricated scene Close.
    /// An active mission is rejected before entering its unavailable dependencies.
    pub fn close(&mut self) -> Result<(), MissionLifecycleError> {
        if self.destructed {
            return Err(MissionLifecycleError::Destructed);
        }
        if self.bytes[0xc] != 0 {
            return Err(MissionLifecycleError::ActiveCloseUnavailable);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn constructor_preserves_nonmember_bytes_and_original_identity_control() {
        let seed = std::array::from_fn(|n| (n as u8).wrapping_mul(13).wrapping_add(7));
        let mut mission = MissionLifecycle::construct(0x7d12_3400, seed);
        for offset in [8, 0x24, 0x2c, 0x30, 0x64, 0x6c, 0x70, 0x74, 0x78, 0x98] {
            assert_eq!(
                &mission.raw_bytes()[offset..offset + 4],
                &seed[offset..offset + 4]
            );
        }
        for offset in [0x14, 0x34, 0x44, 0x54, 0x80, 0xac, 0xbc, 0xcc] {
            assert_eq!(mission.raw_bytes()[offset], 0x7d);
            assert_eq!(
                &mission.raw_bytes()[offset + 1..offset + 4],
                &seed[offset + 1..offset + 4]
            );
        }
        assert_eq!(mission.word(0x90), 1f32.to_bits());
        let before = *mission.raw_bytes();
        mission.close().unwrap();
        assert_eq!(*mission.raw_bytes(), before);
    }
    struct Heap {
        next: u32,
        frees: Vec<u32>,
        present: bool,
        allocations: usize,
        domain: crate::mission_components::MissionMemoryDomainId,
    }
    impl crate::mission_components::MissionMemoryDomain for Heap {
        fn mission_memory_domain(&self) -> crate::mission_components::MissionMemoryDomainId {
            self.domain
        }
    }
    impl MissionAllocator for Heap {
        type Error = ();
        fn allocate(
            &mut self,
            kind: MissionComponentKind,
        ) -> Result<Option<MissionAllocation>, ()> {
            self.allocations += 1;
            let identity = self.next;
            self.next += 0x4000;
            Ok(Some(MissionAllocation {
                domain: self.domain,
                identity,
                bytes: vec![0xa5; kind.allocation_size()],
            }))
        }
        fn free(&mut self, identity: u32) -> Result<(), ()> {
            self.frees.push(identity);
            Ok(())
        }
        fn open_effects_table(&mut self) -> Result<bool, ()> {
            Ok(self.present)
        }
    }
    impl CStrBackend for Heap {
        type Error = ();
        fn allocate_string(
            &mut self,
            _: u32,
        ) -> Result<Option<crate::c_str::StringAllocation>, ()> {
            panic!("empty caches allocate no strings")
        }
        fn free_string(&mut self, _: crate::c_str::StringAllocation) -> Result<(), ()> {
            panic!("empty caches release no strings")
        }
    }
    struct Screen;
    impl crate::scene_resources::SceneScreen for Screen {
        type Error = ();
        fn screen_sx(&mut self) -> Result<i32, ()> {
            Ok(800)
        }
        fn screen_sy(&mut self) -> Result<i32, ()> {
            Ok(600)
        }
    }
    impl DriverSceneScreen for Screen {
        fn viewport_rectangle(&mut self) -> Result<[i32; 4], ()> {
            Ok([10, 20, 810, 620])
        }
    }
    struct MissingImages;
    impl TextureImageSource for MissingImages {
        fn read_image(&mut self, _: &[u8]) -> Result<Vec<u8>, crate::itexture::TextureImageError> {
            Err(crate::itexture::TextureImageError::Missing)
        }
    }
    #[test]
    fn lifecycle_owns_real_scene_and_support_objects_until_last_release() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut arena = crate::model_loading::ModelArena::default();
            let mut resources = crate::scene_resources::SceneResources::default();
            let mut textures = crate::itexture::TextureResources::new().unwrap();
            let mut storage = crate::itexture::CpuTextureStorage::default();
            let mut context = DriverSceneContext {
                arena: &mut arena,
                resources: &mut resources,
                textures: &mut textures,
                storage: &mut storage,
            };
            let mut driver = DriverSceneOwner::default();
            let mut caches = MissionCaches::empty().unwrap();
            let mut heap = Heap {
                next: 0x6d00_0000,
                frees: Vec::new(),
                present: false,
                allocations: 0,
                domain: crate::mission_components::MissionMemoryDomainId::fresh(),
            };
            let mut globals = GameConstructorGlobals {
                word_6d4b1c: 0x12345678,
                word_6bdb10: 9,
            };
            let inputs = MissionInitInputs {
                scene_identity: 0x6e00_0000,
                effects_stack_control: 0x39,
                game: GameConstructorInputs {
                    vector_control: 0x48,
                    embedded_4c_control: 0x57,
                },
            };
            let mut mission = MissionLifecycle::construct(0x6c00_0000, [0; MISSION_BYTES]);
            mission
                .init(
                    &mut driver,
                    &mut context,
                    &mut Screen,
                    &mut MissingImages,
                    NativeSceneSoundDriver::Absent,
                    &mut heap,
                    inputs,
                    &mut globals,
                )
                .unwrap();
            let scene = mission.scene().unwrap();
            assert_eq!(driver.scenes(), &[scene]);
            assert_eq!(heap.allocations, 5);
            assert_eq!(mission.components().count(), 5);
            assert_eq!(globals.word_6bdb10, 0);
            let game = mission
                .components()
                .find(|(component, _)| component.kind() == MissionComponentKind::Game)
                .unwrap()
                .0;
            assert_eq!(game.word(0x44), mission.identity());
            assert_eq!(game.word(0x5c), 0x12345678);
            // Original entry guard returns before any factory/allocation work.
            mission
                .init(
                    &mut driver,
                    &mut context,
                    &mut Screen,
                    &mut MissingImages,
                    NativeSceneSoundDriver::Initialized,
                    &mut heap,
                    inputs,
                    &mut globals,
                )
                .unwrap();
            assert_eq!(heap.allocations, 5);
            let original_domain = heap.domain;
            let before_frees = heap.frees.len();
            heap.domain = crate::mission_components::MissionMemoryDomainId::fresh();
            assert!(mission
                .uninit(&mut caches, &mut driver, &mut context, &mut heap)
                .is_err());
            assert_eq!(heap.frees.len(), before_frees);
            assert_eq!(driver.scenes(), &[scene]);
            heap.domain = original_domain;
            let mut foreign_driver = DriverSceneOwner::default();
            assert!(mission
                .uninit(&mut caches, &mut foreign_driver, &mut context, &mut heap)
                .is_err());
            assert_eq!(heap.frees.len(), before_frees);
            mission
                .destruct(&mut caches, &mut driver, &mut context, &mut heap)
                .unwrap();
            assert!(driver.scenes().is_empty());
            assert!(mission.is_destructed());
            assert!(mission.components().all(|(_, alive)| !alive));
            assert_eq!(
                heap.frees
                    .iter()
                    .copied()
                    .filter(|&x| x != 0)
                    .collect::<Vec<_>>(),
                vec![
                    0x6d00_0000,
                    0x6d00_4000,
                    0x6d00_8000,
                    0x6d00_c000,
                    0x6d01_0000
                ]
            );
            assert!(context.arena.scene(scene).is_err());
        });
    }
    #[test]
    fn present_effects_table_retains_constructor_prefix_without_publishing_it() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut arena = crate::model_loading::ModelArena::default();
            let mut resources = crate::scene_resources::SceneResources::default();
            let mut textures = crate::itexture::TextureResources::new().unwrap();
            let mut storage = crate::itexture::CpuTextureStorage::default();
            let mut context = DriverSceneContext {
                arena: &mut arena,
                resources: &mut resources,
                textures: &mut textures,
                storage: &mut storage,
            };
            let mut driver = DriverSceneOwner::default();
            let mut heap = Heap {
                next: 0x6d00_0000,
                frees: Vec::new(),
                present: true,
                allocations: 0,
                domain: crate::mission_components::MissionMemoryDomainId::fresh(),
            };
            let mut globals = GameConstructorGlobals {
                word_6d4b1c: 0,
                word_6bdb10: 9,
            };
            let mut mission = MissionLifecycle::construct(0x6c00_0000, [0; MISSION_BYTES]);
            let error = mission
                .init(
                    &mut driver,
                    &mut context,
                    &mut Screen,
                    &mut MissingImages,
                    NativeSceneSoundDriver::Absent,
                    &mut heap,
                    MissionInitInputs {
                        scene_identity: 0x6e00_0000,
                        effects_stack_control: 0x39,
                        game: GameConstructorInputs {
                            vector_control: 0x48,
                            embedded_4c_control: 0x57,
                        },
                    },
                    &mut globals,
                )
                .unwrap_err();
            assert_eq!(
                error.phase,
                MissionPhase::Construct(MissionComponentKind::Effects)
            );
            assert_eq!(heap.allocations, 4);
            assert_eq!(mission.word(0x6c), 0);
            assert_eq!(mission.pending_component().unwrap().raw_bytes()[0x30], 0xa5);
            assert_eq!(mission.components().count(), 3);
            assert!(mission.scene().is_some());
            assert_eq!(globals.word_6bdb10, 9);
            assert!(heap.frees.is_empty());
        });
    }
}
