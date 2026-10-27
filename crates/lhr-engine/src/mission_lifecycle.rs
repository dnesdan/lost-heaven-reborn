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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MissionFailure {
    pub phase: MissionPhase,
    pub detail: String,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LifecycleDomains {
    driver: u64,
    arena: crate::script_scheduler::FrameReferenceId,
    resources: u64,
    textures: u64,
    storage: u64,
    memory: crate::mission_components::MissionMemoryDomainId,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ComponentDisposal {
    Ready,
    DestroyEntered,
    Destroyed,
    FreeEntered,
    Released,
}
struct OwnedComponent {
    value: MissionComponent,
    disposal: ComponentDisposal,
}
/// Host terminal progress, independent of the original Init/UnInit phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MissionTerminalPhase {
    NotStarted,
    Components,
    Scene,
    RetainedOwners,
    Complete,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MissionTerminalIssue {
    pub phase: MissionTerminalPhase,
    pub detail: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MissionTerminalProgress {
    pub phase: MissionTerminalPhase,
    pub components_released: usize,
    pub scene_reference_released: bool,
    pub retained: Vec<MissionTerminalIssue>,
}
impl Default for MissionTerminalProgress {
    fn default() -> Self {
        Self {
            phase: MissionTerminalPhase::NotStarted,
            components_released: 0,
            scene_reference_released: false,
            retained: Vec::new(),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MissionTerminalCallError {
    NotInterrupted,
    AlreadyEntered,
    ForeignDomain,
    RetainedOwners,
}

/// Original PC allocation size, independent of the larger PS2 mission.
pub const MISSION_BYTES: usize = 0xdc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MissionLifecycleError {
    /// Active Close requires the game, actors, programs and scene Close owners.
    ActiveCloseUnavailable,
    Destructed,
    TerminalDiscardEntered,
}

pub struct MissionLifecycle {
    identity: u32,
    bytes: [u8; MISSION_BYTES],
    destructed: bool,
    scene: Option<SceneKey>,
    pending_scene: Option<SceneKey>,
    pending_listener: Option<SceneKey>,
    components: Vec<OwnedComponent>,
    pending_component: Option<MissionComponent>,
    rejected_allocation: Option<MissionAllocation>,
    phase: MissionPhase,
    interrupted: bool,
    domains: Option<LifecycleDomains>,
    first_failure: Option<MissionFailure>,
    terminal: MissionTerminalProgress,
    scene_release_entered: bool,
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
            pending_listener: None,
            components: Vec::new(),
            pending_component: None,
            rejected_allocation: None,
            phase: MissionPhase::Complete,
            interrupted: false,
            domains: None,
            first_failure: None,
            terminal: MissionTerminalProgress::default(),
            scene_release_entered: false,
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
    /// Listener retained at the terminal attempt, either behind a surviving
    /// Scene reference or after backend cleanup failed. External cleanup may
    /// subsequently retire this historical identity.
    pub fn pending_listener(&self) -> Option<SceneKey> {
        self.pending_listener
    }
    pub fn pending_component(&self) -> Option<&MissionComponent> {
        self.pending_component.as_ref()
    }
    pub fn rejected_allocation(&self) -> Option<&MissionAllocation> {
        self.rejected_allocation.as_ref()
    }
    /// The boolean records absence of a successful scalar free, not proof of a
    /// live object after a failed destructor/free. Interrupted owners are inspectable.
    pub fn components(&self) -> impl Iterator<Item = (&MissionComponent, bool)> {
        self.components
            .iter()
            .map(|entry| (&entry.value, entry.disposal != ComponentDisposal::Released))
    }
    pub fn phase(&self) -> MissionPhase {
        self.phase
    }
    pub fn interrupted(&self) -> bool {
        self.interrupted
    }
    fn fail(&mut self, detail: impl std::fmt::Debug) -> MissionFailure {
        self.interrupted = true;
        let failure = MissionFailure {
            phase: self.phase,
            detail: format!("{detail:?}"),
        };
        self.first_failure.get_or_insert_with(|| failure.clone());
        failure
    }
    pub fn failure(&self) -> Option<&MissionFailure> {
        self.first_failure.as_ref()
    }
    pub fn terminal_progress(&self) -> &MissionTerminalProgress {
        &self.terminal
    }
    fn available(&self) -> Result<(), MissionFailure> {
        if self.destructed
            || self.interrupted
            || self.terminal.phase != MissionTerminalPhase::NotStarted
        {
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
        driver: &DriverSceneOwner,
    ) -> Result<(), MissionFailure> {
        let domains = LifecycleDomains {
            driver: driver.identity(),
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
        self.init_with_scene_allocator(
            driver,
            context,
            screen,
            images,
            sound,
            allocator,
            inputs,
            globals,
            |_, _| Ok(()),
        )
    }
    /// Same Init protocol with its real driver registry allocation dependency.
    #[allow(clippy::too_many_arguments)]
    fn init_with_scene_allocator<S: DriverSceneScreen, A: MissionAllocator>(
        &mut self,
        driver: &mut DriverSceneOwner,
        context: &mut DriverSceneContext<'_>,
        screen: &mut S,
        images: &mut impl TextureImageSource,
        sound: NativeSceneSoundDriver,
        allocator: &mut A,
        inputs: MissionInitInputs,
        globals: &mut GameConstructorGlobals,
        scene_allocate: impl FnMut(u32, &[SceneKey]) -> Result<(), ()>,
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
        self.bind(context, allocator, driver)?;
        self.phase = MissionPhase::CreateScene;
        if inputs.scene_identity == 0 {
            return Err(self.fail("null observer identity cannot represent the returned Scene"));
        }
        let scene = match driver.create_scene_with_allocator(
            context,
            screen,
            images,
            sound,
            scene_allocate,
        ) {
            Ok(scene) => scene,
            Err(error) => {
                self.pending_scene = error.scene;
                return Err(self.fail(error));
            }
        };
        self.scene = Some(scene);
        self.scene_release_entered = false;
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
                disposal: ComponentDisposal::Ready,
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
                disposal: ComponentDisposal::Ready,
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
        self.bind(context, allocator, driver)?;
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
                entry.disposal == ComponentDisposal::Ready
                    && entry.value.identity() == identity
                    && entry.value.kind() == kind
            }) else {
                return Err(self.fail("component pointer has no matching owned allocation"));
            };
            self.components[index].disposal = ComponentDisposal::DestroyEntered;
            self.components[index]
                .value
                .destroy(allocator)
                .map_err(|e| self.fail(e))?;
            self.components[index].disposal = ComponentDisposal::Destroyed;
            self.phase = MissionPhase::Free(kind);
            self.components[index].disposal = ComponentDisposal::FreeEntered;
            allocator.free(identity).map_err(|e| self.fail(e))?;
            self.components[index].disposal = ComponentDisposal::Released;
            self.put_word(offset, 0);
        }
        if self.word(0x10) != 0 {
            self.phase = MissionPhase::Close;
            self.close().map_err(|e| self.fail(e))?;
            self.phase = MissionPhase::ReleaseScene;
            let scene = self
                .scene
                .ok_or_else(|| self.fail("scene pointer has no matching owned SceneKey"))?;
            self.scene_release_entered = true;
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
    /// One-shot host disposal of proven-owned resources after an interrupted
    /// lifecycle. This never reenters native UnInit or changes its reached phase
    /// and first failure. Each destructor/free is entered at most once. Unknown
    /// constructors, prior failed effects and foreign/raw pointers stay retained.
    pub fn discard_terminal<A: MissionAllocator>(
        &mut self,
        driver: &mut DriverSceneOwner,
        context: &mut DriverSceneContext<'_>,
        allocator: &mut A,
    ) -> Result<(), MissionTerminalCallError>
    where
        A::Error: std::fmt::Debug,
    {
        if self.terminal.phase != MissionTerminalPhase::NotStarted {
            return Err(MissionTerminalCallError::AlreadyEntered);
        }
        if !self.interrupted {
            return Err(MissionTerminalCallError::NotInterrupted);
        }
        // Admission failures perform no cleanup and do not consume the attempt.
        self.bind(context, allocator, driver)
            .map_err(|_| MissionTerminalCallError::ForeignDomain)?;
        if let Some(scene) = self.scene {
            if !self.scene_release_entered {
                driver
                    .validate_scene_domain(context.arena, context.resources, scene)
                    .map_err(|_| MissionTerminalCallError::ForeignDomain)?;
            }
        }
        self.terminal.phase = MissionTerminalPhase::Components;
        if self.bytes[0x0c] != 0 {
            self.retain_terminal("active mission requires unavailable GameDone/Close ownership");
            self.terminal.phase = MissionTerminalPhase::RetainedOwners;
            return Err(MissionTerminalCallError::RetainedOwners);
        }
        for index in 0..self.components.len() {
            let entry = &self.components[index];
            if entry.disposal == ComponentDisposal::Released {
                continue;
            }
            if entry.disposal != ComponentDisposal::Ready {
                self.retain_terminal(format!(
                    "component {:?} {:#x} prior {:?}; native effect must not be replayed",
                    entry.value.kind(),
                    entry.value.identity(),
                    entry.disposal
                ));
                continue;
            }
            let kind = entry.value.kind();
            let identity = entry.value.identity();
            self.components[index].disposal = ComponentDisposal::DestroyEntered;
            if let Err(error) = self.components[index].value.destroy(allocator) {
                self.retain_terminal(format!(
                    "component {kind:?} destruction retained: {error:?}"
                ));
                continue;
            }
            self.components[index].disposal = ComponentDisposal::Destroyed;
            self.components[index].disposal = ComponentDisposal::FreeEntered;
            if let Err(error) = allocator.free(identity) {
                self.retain_terminal(format!(
                    "component {kind:?} scalar free has unresolved outcome: {error:?}"
                ));
                continue;
            }
            self.components[index].disposal = ComponentDisposal::Released;
            self.terminal.components_released += 1;
            let offset = component_offset(kind);
            if self.word(offset) == identity {
                self.put_word(offset, 0);
            }
        }
        if let Some(pending) = &self.pending_component {
            self.retain_terminal(format!("incomplete {:?} constructor {:#x}; pending stream/partial destruction contract unavailable",
                pending.kind(), pending.identity()));
        }
        if self.rejected_allocation.is_some() {
            self.retain_terminal("rejected allocation ownership/extent remains unresolved");
        }
        for kind in [
            MissionComponentKind::Tshift,
            MissionComponentKind::WebPath,
            MissionComponentKind::Roads,
            MissionComponentKind::Effects,
            MissionComponentKind::Game,
        ] {
            let identity = self.word(component_offset(kind));
            if identity != 0
                && !self
                    .components
                    .iter()
                    .any(|entry| entry.value.identity() == identity && entry.value.kind() == kind)
            {
                self.retain_terminal(format!("unowned {kind:?} pointer {identity:#x} retained"));
            }
        }
        self.terminal.phase = MissionTerminalPhase::Scene;
        if self.scene_release_entered && self.scene.is_some() {
            self.retain_terminal(
                "prior scene release has unresolved outcome; release must not be replayed",
            );
        } else if let Some(scene) = self.scene {
            self.scene_release_entered = true;
            match driver.release_scene(context, scene) {
                Ok(()) => {
                    self.scene = None;
                    self.put_word(0x10, 0);
                    self.terminal.scene_reference_released = true;
                }
                Err(error) => self.retain_terminal(format!("scene release retained: {error:?}")),
            }
        }
        if let Some(scene) = self.pending_scene {
            // The factory's actual pending Scene may be fully constructed (for
            // example viewport failure) or incomplete. The existing resource
            // owner decides admission; no constructor/destructor is fabricated.
            match context.resources.release_scene(
                context.arena,
                scene,
                context.textures,
                context.storage,
            ) {
                Ok(_) => {
                    self.pending_scene = None;
                    self.terminal.scene_reference_released = true;
                    if context.arena.scene(scene).is_ok() {
                        // Release may only have decremented a retained Scene.
                        // Its listener follows actual destruction, not this
                        // owner's successful release of one reference.
                        match driver
                            .host_sound_backend()
                            .map(|backend| backend.listeners())
                            .transpose()
                        {
                            Ok(Some(listeners)) if listeners.contains(&scene) => {
                                self.pending_listener = Some(scene);
                                self.retain_terminal("unregistered scene listener retained behind surviving external Scene reference");
                            }
                            Err(error) => {
                                self.pending_listener = Some(scene);
                                self.retain_terminal(format!(
                                    "surviving Scene listener ownership query failed: {error:?}"
                                ));
                            }
                            _ => {}
                        }
                    } else if let Err(error) = driver.release_unregistered_scene_listener(scene) {
                        self.pending_listener = Some(scene);
                        self.retain_terminal(format!(
                            "unregistered scene listener retained: {error:?}"
                        ));
                    }
                }
                Err(error) => self.retain_terminal(format!("pending scene retained: {error:?}")),
            }
        }
        if self.word(0x68) != 0 {
            self.retain_terminal("unknown68 owner retained");
        }
        for offset in [0xcc, 0xbc, 0xac, 0x80, 0x54, 0x44, 0x34, 0x14] {
            if self.word(offset + 4) != 0 {
                self.retain_terminal(format!("unknown mission vector at {offset:#x} retained"));
            }
        }
        if self.terminal.retained.is_empty() {
            self.terminal.phase = MissionTerminalPhase::Complete;
            Ok(())
        } else {
            self.terminal.phase = MissionTerminalPhase::RetainedOwners;
            Err(MissionTerminalCallError::RetainedOwners)
        }
    }
    fn retain_terminal(&mut self, detail: impl Into<String>) {
        self.terminal.retained.push(MissionTerminalIssue {
            phase: self.terminal.phase,
            detail: detail.into(),
        });
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
        if self.terminal.phase != MissionTerminalPhase::NotStarted {
            return Err(MissionLifecycleError::TerminalDiscardEntered);
        }
        if self.bytes[0xc] != 0 {
            return Err(MissionLifecycleError::ActiveCloseUnavailable);
        }
        Ok(())
    }
}

fn component_offset(kind: MissionComponentKind) -> usize {
    match kind {
        MissionComponentKind::Tshift => 0x28,
        MissionComponentKind::WebPath => 0x2c,
        MissionComponentKind::Roads => 0x30,
        MissionComponentKind::Effects => 0x6c,
        MissionComponentKind::Game => 0x24,
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
    struct FaultHeap {
        domain: crate::mission_components::MissionMemoryDomainId,
        next: u32,
        live: std::collections::BTreeSet<u32>,
        free_calls: Vec<u32>,
        fail_allocate: Option<MissionComponentKind>,
        invalid_allocate: Option<MissionComponentKind>,
        fail_free_at: Option<usize>,
        effects_present: bool,
        fail_effects_open: bool,
        fail_effects_read: bool,
        effects_bytes: Option<Vec<u8>>,
    }
    impl Default for FaultHeap {
        fn default() -> Self {
            Self {
                domain: crate::mission_components::MissionMemoryDomainId::fresh(),
                next: 0x6d00_0000,
                live: Default::default(),
                free_calls: Vec::new(),
                fail_allocate: None,
                invalid_allocate: None,
                fail_free_at: None,
                effects_present: false,
                fail_effects_open: false,
                fail_effects_read: false,
                effects_bytes: None,
            }
        }
    }
    impl crate::mission_components::MissionMemoryDomain for FaultHeap {
        fn mission_memory_domain(&self) -> crate::mission_components::MissionMemoryDomainId {
            self.domain
        }
    }
    impl MissionAllocator for FaultHeap {
        type Error = &'static str;
        fn allocate(
            &mut self,
            kind: MissionComponentKind,
        ) -> Result<Option<MissionAllocation>, Self::Error> {
            if self.fail_allocate == Some(kind) {
                return Err("injected component allocation");
            }
            let identity = self.next;
            self.next += 0x4000;
            self.live.insert(identity);
            Ok(Some(MissionAllocation {
                domain: if self.invalid_allocate == Some(kind) {
                    crate::mission_components::MissionMemoryDomainId::fresh()
                } else {
                    self.domain
                },
                identity,
                bytes: vec![0xa5; kind.allocation_size()],
            }))
        }
        fn free(&mut self, identity: u32) -> Result<(), Self::Error> {
            self.free_calls.push(identity);
            if identity != 0 {
                assert!(
                    self.live.remove(&identity),
                    "allocation must not be freed twice"
                );
            }
            // Failure deliberately occurs AFTER the backing was freed. A caller
            // must retain the uncertain observation instead of repeating it.
            if self.fail_free_at == Some(self.free_calls.len()) {
                return Err("injected ambiguous scalar free");
            }
            Ok(())
        }
        fn open_effects_table(&mut self) -> Result<bool, Self::Error> {
            if self.fail_effects_open {
                Err("injected effects open")
            } else {
                Ok(self.effects_present)
            }
        }
        fn read_effects_table(&mut self) -> Result<Option<Vec<u8>>, Self::Error> {
            if self.fail_effects_read {
                Err("injected effects read")
            } else {
                Ok(self.effects_bytes.take())
            }
        }
    }
    impl CStrBackend for FaultHeap {
        type Error = &'static str;
        fn allocate_string(
            &mut self,
            _: u32,
        ) -> Result<Option<crate::c_str::StringAllocation>, Self::Error> {
            panic!("empty caches allocate no strings")
        }
        fn free_string(&mut self, _: crate::c_str::StringAllocation) -> Result<(), Self::Error> {
            panic!("empty caches release no strings")
        }
    }
    #[derive(Default)]
    struct FaultScreen {
        fail_size: bool,
        fail_viewport: bool,
    }
    impl crate::scene_resources::SceneScreen for FaultScreen {
        type Error = &'static str;
        fn screen_sx(&mut self) -> Result<i32, Self::Error> {
            if self.fail_size {
                Err("injected constructor screen")
            } else {
                Ok(800)
            }
        }
        fn screen_sy(&mut self) -> Result<i32, Self::Error> {
            if self.fail_size {
                Err("injected constructor screen")
            } else {
                Ok(600)
            }
        }
    }
    impl DriverSceneScreen for FaultScreen {
        fn viewport_rectangle(&mut self) -> Result<[i32; 4], Self::Error> {
            if self.fail_viewport {
                Err("injected viewport")
            } else {
                Ok([0, 0, 800, 600])
            }
        }
    }
    fn with_fault_fixture(
        run: impl FnOnce(
            &mut MissionLifecycle,
            &mut DriverSceneOwner,
            &mut DriverSceneContext<'_>,
            &mut FaultHeap,
        ),
    ) {
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
            run(
                &mut MissionLifecycle::construct(0x6c00_0000, [0; MISSION_BYTES]),
                &mut DriverSceneOwner::default(),
                &mut context,
                &mut FaultHeap::default(),
            );
        });
    }
    fn fault_init(
        mission: &mut MissionLifecycle,
        driver: &mut DriverSceneOwner,
        context: &mut DriverSceneContext<'_>,
        heap: &mut FaultHeap,
        screen: &mut FaultScreen,
    ) -> Result<(), MissionFailure> {
        mission.init(
            driver,
            context,
            screen,
            &mut MissingImages,
            NativeSceneSoundDriver::Absent,
            heap,
            MissionInitInputs {
                scene_identity: 0x6e00_0000,
                effects_stack_control: 0x39,
                game: GameConstructorInputs {
                    vector_control: 0x48,
                    embedded_4c_control: 0x57,
                },
            },
            &mut GameConstructorGlobals {
                word_6d4b1c: 0,
                word_6bdb10: 9,
            },
        )
    }
    #[test]
    fn terminal_disposal_releases_every_proven_init_allocation_prefix_once() {
        for (count, kind) in [
            MissionComponentKind::Tshift,
            MissionComponentKind::WebPath,
            MissionComponentKind::Roads,
            MissionComponentKind::Effects,
            MissionComponentKind::Game,
        ]
        .into_iter()
        .enumerate()
        {
            with_fault_fixture(|mission, driver, context, heap| {
                heap.fail_allocate = Some(kind);
                let failure =
                    fault_init(mission, driver, context, heap, &mut FaultScreen::default())
                        .unwrap_err();
                assert_eq!(failure.phase, MissionPhase::Allocate(kind));
                assert_eq!(mission.failure(), Some(&failure));
                assert_eq!(heap.live.len(), count);
                let scene = mission.scene().unwrap();
                mission.discard_terminal(driver, context, heap).unwrap();
                assert_eq!(
                    mission.terminal_progress().phase,
                    MissionTerminalPhase::Complete
                );
                assert_eq!(mission.terminal_progress().components_released, count);
                assert_eq!(mission.phase(), failure.phase);
                assert_eq!(mission.failure(), Some(&failure));
                assert!(!mission.is_destructed()); // no original destructor credit
                assert_eq!(
                    mission.close(),
                    Err(MissionLifecycleError::TerminalDiscardEntered)
                );
                assert!(heap.live.is_empty());
                assert!(driver.scenes().is_empty());
                assert!(context.arena.scene(scene).is_err());
                let frees = heap.free_calls.clone();
                assert_eq!(
                    mission.discard_terminal(driver, context, heap),
                    Err(MissionTerminalCallError::AlreadyEntered)
                );
                assert_eq!(heap.free_calls, frees);
            });
        }
    }
    #[test]
    fn terminal_scene_factory_cleanup_obeys_completed_versus_incomplete_owner_contract() {
        for incomplete in [false, true] {
            with_fault_fixture(|mission, driver, context, heap| {
                let mut screen = FaultScreen {
                    fail_size: incomplete,
                    fail_viewport: !incomplete,
                };
                let failure = fault_init(mission, driver, context, heap, &mut screen).unwrap_err();
                let scene = mission.pending_scene().unwrap();
                assert_eq!(failure.phase, MissionPhase::CreateScene);
                let result = mission.discard_terminal(driver, context, heap);
                if incomplete {
                    assert_eq!(result, Err(MissionTerminalCallError::RetainedOwners));
                    assert_eq!(mission.pending_scene(), Some(scene));
                    assert!(mission
                        .terminal_progress()
                        .retained
                        .iter()
                        .any(|issue| issue.detail.contains("pending scene retained")));
                    assert!(context.arena.scene(scene).is_ok());
                } else {
                    result.unwrap();
                    assert_eq!(mission.pending_scene(), None);
                    assert!(context.arena.scene(scene).is_err());
                }
                assert_eq!(mission.failure(), Some(&failure));
                assert_eq!(mission.phase(), failure.phase);
                assert!(heap.free_calls.is_empty());
                assert_eq!(
                    mission.discard_terminal(driver, context, heap),
                    Err(MissionTerminalCallError::AlreadyEntered)
                );
            });
        }
    }
    #[test]
    fn incomplete_effects_are_retained_while_independent_prefix_and_scene_are_released() {
        for phase in 0..4 {
            with_fault_fixture(|mission, driver, context, heap| {
                heap.effects_present = true;
                heap.fail_effects_open = phase == 0;
                heap.fail_effects_read = phase == 1;
                if phase == 3 {
                    heap.effects_bytes = Some(vec![1, 2, 3]);
                }
                let failure =
                    fault_init(mission, driver, context, heap, &mut FaultScreen::default())
                        .unwrap_err();
                assert_eq!(
                    failure.phase,
                    MissionPhase::Construct(MissionComponentKind::Effects)
                );
                let pending = mission.pending_component().unwrap().identity();
                assert_eq!(
                    mission.discard_terminal(driver, context, heap),
                    Err(MissionTerminalCallError::RetainedOwners)
                );
                assert_eq!(heap.live.iter().copied().collect::<Vec<_>>(), [pending]);
                assert!(!heap.free_calls.contains(&pending));
                assert_eq!(mission.pending_component().unwrap().identity(), pending);
                assert_eq!(mission.terminal_progress().components_released, 3);
                assert!(mission.terminal_progress().scene_reference_released);
                assert!(driver.scenes().is_empty());
                assert_eq!(mission.failure(), Some(&failure));
            });
        }
    }
    #[test]
    fn terminal_disposal_never_replays_prior_uninit_destroy_or_ambiguous_free() {
        for failed_free in [false, true] {
            with_fault_fixture(|mission, driver, context, heap| {
                fault_init(mission, driver, context, heap, &mut FaultScreen::default()).unwrap();
                let first = mission.components().next().unwrap().0.identity();
                heap.fail_free_at = Some(heap.free_calls.len() + if failed_free { 2 } else { 1 });
                let failure = mission
                    .uninit(&mut MissionCaches::empty().unwrap(), driver, context, heap)
                    .unwrap_err();
                assert_eq!(
                    failure.phase,
                    if failed_free {
                        MissionPhase::Free(MissionComponentKind::Tshift)
                    } else {
                        MissionPhase::Destroy(MissionComponentKind::Tshift)
                    }
                );
                assert_eq!(
                    mission.discard_terminal(driver, context, heap),
                    Err(MissionTerminalCallError::RetainedOwners)
                );
                assert_eq!(
                    heap.free_calls
                        .iter()
                        .filter(|&&identity| identity == first)
                        .count(),
                    usize::from(failed_free)
                );
                assert_eq!(mission.terminal_progress().components_released, 4);
                assert!(mission.terminal_progress().scene_reference_released);
                assert!(mission
                    .terminal_progress()
                    .retained
                    .iter()
                    .any(|issue| issue.detail.contains("must not be replayed")));
                assert_eq!(mission.failure(), Some(&failure));
                assert_eq!(mission.phase(), failure.phase);
                let frees = heap.free_calls.clone();
                assert_eq!(
                    mission.discard_terminal(driver, context, heap),
                    Err(MissionTerminalCallError::AlreadyEntered)
                );
                assert_eq!(heap.free_calls, frees);
                assert!(mission
                    .uninit(&mut MissionCaches::empty().unwrap(), driver, context, heap)
                    .is_err());
                assert_eq!(heap.free_calls, frees);
            });
        }
    }
    #[test]
    fn foreign_terminal_domains_are_rejected_before_effects_without_consuming_the_attempt() {
        with_fault_fixture(|mission, driver, context, heap| {
            heap.fail_allocate = Some(MissionComponentKind::Roads);
            let failure = fault_init(mission, driver, context, heap, &mut FaultScreen::default())
                .unwrap_err();
            let before = heap.free_calls.clone();
            let old_domain = heap.domain;
            heap.domain = crate::mission_components::MissionMemoryDomainId::fresh();
            assert_eq!(
                mission.discard_terminal(driver, context, heap),
                Err(MissionTerminalCallError::ForeignDomain)
            );
            heap.domain = old_domain;
            assert_eq!(
                mission.discard_terminal(&mut DriverSceneOwner::default(), context, heap),
                Err(MissionTerminalCallError::ForeignDomain)
            );
            assert_eq!(
                mission.terminal_progress().phase,
                MissionTerminalPhase::NotStarted
            );
            assert_eq!(heap.free_calls, before);
            assert_eq!(mission.failure(), Some(&failure));
            mission.discard_terminal(driver, context, heap).unwrap();
            assert!(heap.live.is_empty());
        });
    }
    #[test]
    fn rejected_allocations_and_active_close_remain_explicit_unresolved_owners() {
        with_fault_fixture(|mission, driver, context, heap| {
            heap.invalid_allocate = Some(MissionComponentKind::Roads);
            let failure = fault_init(mission, driver, context, heap, &mut FaultScreen::default())
                .unwrap_err();
            let rejected = mission.rejected_allocation.as_ref().unwrap().identity;
            assert_eq!(
                mission.discard_terminal(driver, context, heap),
                Err(MissionTerminalCallError::RetainedOwners)
            );
            assert!(!heap.free_calls.contains(&rejected));
            assert_eq!(heap.live.iter().copied().collect::<Vec<_>>(), [rejected]);
            assert_eq!(mission.failure(), Some(&failure));
        });
        with_fault_fixture(|mission, driver, context, heap| {
            assert_eq!(
                mission.discard_terminal(driver, context, heap),
                Err(MissionTerminalCallError::NotInterrupted)
            );
            fault_init(mission, driver, context, heap, &mut FaultScreen::default()).unwrap();
            mission.bytes[0x0c] = 1;
            let failure = mission
                .uninit(&mut MissionCaches::empty().unwrap(), driver, context, heap)
                .unwrap_err();
            let before = heap.free_calls.clone();
            assert_eq!(
                mission.discard_terminal(driver, context, heap),
                Err(MissionTerminalCallError::RetainedOwners)
            );
            assert_eq!(heap.free_calls, before);
            assert_eq!(heap.live.len(), 5);
            assert_eq!(mission.failure(), Some(&failure));
        });
    }
    #[test]
    fn terminal_own_effect_failures_retain_only_the_uncertain_component_and_keep_original_error() {
        let destructor_calls = [1, 1, 2, 3];
        let preceding_calls = [0, 2, 4, 7];
        for index in 0..4 {
            for scalar_free in [false, true] {
                with_fault_fixture(|mission, driver, context, heap| {
                    heap.fail_allocate = Some(MissionComponentKind::Game);
                    let initial =
                        fault_init(mission, driver, context, heap, &mut FaultScreen::default())
                            .unwrap_err();
                    let identity = mission.components().nth(index).unwrap().0.identity();
                    heap.fail_free_at = Some(
                        heap.free_calls.len()
                            + preceding_calls[index]
                            + if scalar_free {
                                destructor_calls[index] + 1
                            } else {
                                1
                            },
                    );
                    assert_eq!(
                        mission.discard_terminal(driver, context, heap),
                        Err(MissionTerminalCallError::RetainedOwners)
                    );
                    assert_eq!(mission.terminal_progress().components_released, 3);
                    assert!(mission.terminal_progress().scene_reference_released);
                    assert_eq!(mission.failure(), Some(&initial));
                    assert_eq!(mission.phase(), initial.phase);
                    assert_eq!(
                        heap.free_calls
                            .iter()
                            .filter(|&&value| value == identity)
                            .count(),
                        usize::from(scalar_free)
                    );
                    assert_eq!(heap.live.contains(&identity), !scalar_free);
                    assert_eq!(mission.components().filter(|(_, alive)| *alive).count(), 1);
                    let frees = heap.free_calls.clone();
                    assert_eq!(
                        mission.discard_terminal(driver, context, heap),
                        Err(MissionTerminalCallError::AlreadyEntered)
                    );
                    assert_eq!(heap.free_calls, frees);
                });
            }
        }
    }
    #[test]
    fn terminal_never_retries_a_preexisting_scene_release_failure_even_after_dependency_changes() {
        with_fault_fixture(|mission, driver, context, heap| {
            fault_init(mission, driver, context, heap, &mut FaultScreen::default()).unwrap();
            let scene = mission.scene().unwrap();
            let nested = driver
                .create_scene(
                    context,
                    &mut FaultScreen::default(),
                    &mut MissingImages,
                    NativeSceneSoundDriver::Absent,
                )
                .unwrap();
            context.arena.scene_add_frame(scene, nested.root()).unwrap();
            let initial = mission
                .uninit(&mut MissionCaches::empty().unwrap(), driver, context, heap)
                .unwrap_err();
            assert_eq!(initial.phase, MissionPhase::ReleaseScene);
            assert!(initial
                .detail
                .contains("foreign derived scene-frame resources"));
            assert!(heap.live.is_empty());
            // A changing dependency must not turn host terminal disposal into a
            // replay of a partially entered native scene destructor.
            context
                .arena
                .scene_delete_frame(scene, nested.root())
                .unwrap();
            let scenes = driver.scenes().to_vec();
            let frees = heap.free_calls.clone();
            assert_eq!(
                mission.discard_terminal(driver, context, heap),
                Err(MissionTerminalCallError::RetainedOwners)
            );
            assert_eq!(mission.scene(), Some(scene));
            assert_eq!(driver.scenes(), scenes);
            assert_eq!(heap.free_calls, frees);
            assert!(!mission.terminal_progress().scene_reference_released);
            assert!(mission
                .terminal_progress()
                .retained
                .iter()
                .any(|issue| issue.detail.contains("prior scene release")));
            assert_eq!(mission.failure(), Some(&initial));
        });
    }
    #[test]
    fn terminal_retained_unregistered_scene_keeps_listener_until_last_alias_destruction() {
        with_fault_fixture(|mission, driver, context, heap| {
            let audio = crate::runtime_audio::RuntimeAudio::default();
            driver
                .bind_host_sound_backend(context.arena, &audio)
                .unwrap();
            let failure = mission
                .init_with_scene_allocator(
                    driver,
                    context,
                    &mut FaultScreen::default(),
                    &mut MissingImages,
                    NativeSceneSoundDriver::Initialized,
                    heap,
                    MissionInitInputs {
                        scene_identity: 0x6e00_0000,
                        effects_stack_control: 0x39,
                        game: GameConstructorInputs {
                            vector_control: 0x48,
                            embedded_4c_control: 0x57,
                        },
                    },
                    &mut GameConstructorGlobals {
                        word_6d4b1c: 0,
                        word_6bdb10: 9,
                    },
                    |capacity, scenes| {
                        assert_eq!(capacity, 32);
                        assert!(scenes.is_empty());
                        Err(())
                    },
                )
                .unwrap_err();
            assert_eq!(failure.phase, MissionPhase::CreateScene);
            assert!(failure.detail.contains("Registration"));
            assert!(driver.allocation_incomplete());
            assert!(driver.scenes().is_empty());
            let scene = mission.pending_scene().unwrap();
            assert_eq!(
                driver.host_sound_backend().unwrap().listeners().unwrap(),
                [scene]
            );
            context.arena.retain(scene.root()).unwrap();
            assert_eq!(context.arena.frame(scene.root()).unwrap().references, 2);
            assert_eq!(
                mission.discard_terminal(driver, context, heap),
                Err(MissionTerminalCallError::RetainedOwners)
            );
            assert!(mission.terminal_progress().scene_reference_released);
            assert_eq!(mission.pending_scene(), None);
            assert_eq!(mission.pending_listener(), Some(scene));
            assert!(mission
                .terminal_progress()
                .retained
                .iter()
                .any(|issue| issue.detail.contains("surviving external Scene reference")));
            assert_eq!(context.arena.frame(scene.root()).unwrap().references, 1);
            assert!(context.arena.scene(scene).is_ok());
            assert_eq!(
                driver.host_sound_backend().unwrap().listeners().unwrap(),
                [scene]
            );
            assert_eq!(mission.failure(), Some(&failure));
            assert_eq!(mission.phase(), failure.phase);
            assert!(heap.free_calls.is_empty());
            assert_eq!(
                mission.discard_terminal(driver, context, heap),
                Err(MissionTerminalCallError::AlreadyEntered)
            );
            assert_eq!(context.arena.frame(scene.root()).unwrap().references, 1);
            // The external owner releases the final reference through the real
            // unregistered Scene owner, then performs its listener cleanup.
            context
                .resources
                .release_scene(context.arena, scene, context.textures, context.storage)
                .unwrap();
            assert!(context.arena.scene(scene).is_err());
            assert_eq!(
                driver.host_sound_backend().unwrap().listeners().unwrap(),
                [scene]
            );
            driver.release_unregistered_scene_listener(scene).unwrap();
            assert!(driver
                .host_sound_backend()
                .unwrap()
                .listeners()
                .unwrap()
                .is_empty());
            assert_eq!(mission.failure(), Some(&failure));
        });
    }
}
