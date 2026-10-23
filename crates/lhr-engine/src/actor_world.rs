//! Owned base actors and their distinct Game source collections.
//!
//! Base C_actor construction/Init/destruction and source-only C_detector own
//! their actual embedded program lifetime. This is not detector GameInit,
//! compiled instruction execution, a mission loader or an LS3DF scene constructor.
//! Names C_actor, AddActorToGlobal and C_mission::UnloadActor are corroborated
//! by original PS2 symbols; field names below remain descriptive PC projections.
//! Native allocation is an explicit dependency, separate from Rust storage.
//! Host work/storage bounds allow 65,536 total constructed incarnations (dead
//! tombstones remain inspectable) and 65,536 entries per source. These are host
//! admission limits, not native actor limits. Unsupported native allocator
//! exceptions, allocator callback reentry into collection mutation, other derived virtuals and
//! nonzero manager cleanup flags have no implicit successful implementation.
use crate::frame_lookup::{FrameLookupBackend, SceneRoots};
use crate::original_program::{
    CProgram, OwnerFrameName, ProgramBackend, ProgramContext, ProgramError, ProgramReader,
    ProgramReplacement, ProgramSeed,
};
use crate::script_lookup::{ActorLifetime, LookupError, LookupWorldBackend, MAX_ACTOR_SOURCES};
use crate::script_scheduler::{ActorReferenceId, FrameReferenceId};

/// Bytes/fields left untouched by the base constructor. There is deliberately
/// no default: the allocator/caller must supply the actual storage seed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseActorSeed {
    pub word_0e: u16,
    pub actor_id: u32,
    pub state_1c: u32,
    pub bytes_21_5f: [u8; 63],
    pub bytes_64_65: [u8; 2],
    pub byte_67: u8,
    pub release_frame: u8,
    pub bytes_6d_6f: [u8; 3],
}

/// Typed projection of all 0x70 bytes of native base storage. Frame identities
/// and the base vtable tag replace native pointers; no host pointer is exposed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseActorState {
    pub vtable: ActorVtable,
    pub flags: u32,
    pub user_data: u32,
    pub references: u16,
    pub word_0e: u16,
    pub word_10: u32,
    pub word_14: u32,
    pub actor_id: u32,
    pub state_1c: u32,
    pub byte_20: u8,
    pub bytes_21_5f: [u8; 63],
    pub word_60: u32,
    pub bytes_64_65: [u8; 2],
    pub byte_66: u8,
    pub byte_67: u8,
    pub frame: Option<FrameReferenceId>,
    pub release_frame: u8,
    pub bytes_6d_6f: [u8; 3],
}
impl BaseActorState {
    fn construct(seed: BaseActorSeed) -> Self {
        Self {
            vtable: ActorVtable::Base,
            flags: 0,
            user_data: 0,
            references: 1,
            word_0e: seed.word_0e,
            word_10: 1,
            word_14: 1,
            actor_id: seed.actor_id,
            state_1c: seed.state_1c,
            byte_20: 0,
            bytes_21_5f: seed.bytes_21_5f,
            word_60: 0,
            bytes_64_65: seed.bytes_64_65,
            byte_66: 0,
            byte_67: seed.byte_67,
            frame: None,
            release_frame: seed.release_frame,
            bytes_6d_6f: seed.bytes_6d_6f,
        }
    }
}
/// Evidenced primary vtable identities; opaque tags supply no derived behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorVtable {
    Base,
    Detector,
    Opaque(u32),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorPhase {
    Constructing,
    Ready,
    Initializing,
    InitializationInterrupted,
    Loading,
    LoadingInterrupted,
    Destroying,
    DestructionInterrupted,
    Dead,
}
#[derive(Debug)]
struct OwnedActor {
    lifetime: ActorLifetime,
    state: BaseActorState,
    phase: ActorPhase,
    detector: Option<DetectorState>,
}
/// Untouched native detector storage, including the embedded program seed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetectorSeed {
    pub program: ProgramSeed,
    pub bytes_d8_ef: [u8; 24],
    pub word_f4: u32,
    pub bytes_105_107: [u8; 3],
    pub bytes_118_127: [u8; 16],
}
/// Actual source-only C_detector storage owned by the same actor incarnation.
/// The optional program is taken only during a serialized payload operation;
/// actor phase prevents same-actor reentry while context reads the live owner.
#[derive(Debug)]
pub struct DetectorState {
    program: Option<CProgram>,
    pub bytes_d8_ef: [u8; 24],
    pub word_f0: u32,
    pub word_f4: u32,
    pub words_f8_100: [u32; 3],
    pub byte_104: u8,
    pub bytes_105_107: [u8; 3],
    pub words_108_114: [u32; 4],
    pub bytes_118_127: [u8; 16],
}
impl DetectorState {
    pub fn program(&self) -> Option<&CProgram> {
        self.program.as_ref()
    }
    pub fn data_d8(&self) -> u32 {
        u32::from_le_bytes(self.bytes_d8_ef[..4].try_into().unwrap())
    }
}
/// Read actual current actor attachment and frame full name at the replacement
/// comparison point. Mission path comes from the caller's mission owner; no
/// model root or metadata name is promoted to mission context.
pub struct ActorProgramContext<'a> {
    pub world: &'a ActorWorld,
    pub frames: &'a dyn FrameLookupBackend,
    pub mission_path: Option<&'a [u8]>,
}
impl ProgramContext for ActorProgramContext<'_> {
    fn default_c_locale(&self) -> bool {
        self.frames.default_c_locale()
    }
    fn mission_path(&mut self) -> Result<Option<&[u8]>, String> {
        Ok(self.mission_path)
    }
    fn owner_frame_name(
        &mut self,
        owner: Option<ActorReferenceId>,
    ) -> Result<OwnerFrameName<'_>, String> {
        let Some(owner) = owner else {
            return Ok(OwnerFrameName::NoFrame);
        };
        let state = self.world.state(owner).map_err(|e| format!("{e:?}"))?;
        let Some(frame) = state.frame else {
            return Ok(OwnerFrameName::NoFrame);
        };
        let view = self.frames.frame(frame).map_err(|e| format!("{e:?}"))?;
        Ok(OwnerFrameName::Frame(view.name))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorSource {
    Static,
    Primary,
    Active,
}
/// Opaque native backing allocation retained by the supplied allocator owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorStorageId(pub u32);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceBacking {
    pub storage: Option<ActorStorageId>,
    pub capacity: usize,
}
#[derive(Debug)]
struct ActorCollection {
    values: Vec<Option<ActorReferenceId>>,
    backing: SourceBacking,
    /// On a failed free the new native allocation still belongs to the backend.
    /// The old source/header has not committed. Further insertion is rejected.
    pending: Option<SourceBacking>,
}
impl ActorCollection {
    fn new(backing: SourceBacking) -> Result<Self, ActorWorldError> {
        if backing.capacity > MAX_ACTOR_SOURCES
            || (backing.capacity != 0 && backing.storage.is_none())
        {
            return Err(ActorWorldError::InvalidBacking);
        }
        let mut values = Vec::new();
        values
            .try_reserve(backing.capacity)
            .map_err(|_| ActorWorldError::HostAllocation)?;
        Ok(Self {
            values,
            backing,
            pending: None,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActorWorldError {
    NullIdentity,
    DuplicateIdentity,
    InvalidBacking,
    HostAllocation,
    SourceLimit,
    StaleActor(ActorReferenceId),
    Busy(ActorReferenceId),
    IncompleteGrowth(ActorSource),
    UnsupportedKind(u32),
    UnsupportedManagerFlag,
    MissingAttachedFrame,
    UnsupportedVtable,
    NotDetector,
    Program(ProgramError),
    Effect { stage: &'static str, detail: String },
}
#[derive(Debug, PartialEq, Eq)]
pub struct ActorCreationError {
    /// An interrupted constructor has already created this owned identity.
    pub actor: Option<ActorReferenceId>,
    pub source: ActorWorldError,
}
/// Allocate/free the original vector backing. Calls are serialized and must not
/// reenter source mutation. Reads of `world` observe the original header until
/// free completes; context primary commits before active allocation begins.
/// Errors are host availability failures, not recovered native allocator returns.
/// A failed free must retain both allocations for explicit backend recovery.
pub trait ActorCollectionBackend {
    fn allocate_collection(
        &mut self,
        world: &ActorWorld,
        source: ActorSource,
        bytes: u32,
    ) -> Result<ActorStorageId, String>;
    fn free_collection(
        &mut self,
        world: &ActorWorld,
        source: ActorSource,
        old: Option<ActorStorageId>,
    ) -> Result<(), String>;
}
/// Distinguishes a null frame userdata pointer from nonnull userdata whose first
/// actor word is null. Both are accepted by base Init.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameActorData {
    Null,
    Present(Option<ActorReferenceId>),
}
/// Actual frame operations; flags 0x40 are not the script lookup's 0x4ffff.
/// Callbacks may synchronously edit actor state and sources in this live world.
/// Reentering construction/Init/destruction of the same actor is rejected.
/// Backend errors preserve the exact already-applied prefix; they never imply
/// successful frame attachment, virtual release, or gameplay initialization.
pub trait ActorFrameBackend {
    fn actor_data(
        &mut self,
        world: &mut ActorWorld,
        frame: FrameReferenceId,
    ) -> Result<FrameActorData, String>;
    fn clear_actor_data(
        &mut self,
        world: &mut ActorWorld,
        frame: FrameReferenceId,
    ) -> Result<(), String>;
    fn find_child(
        &mut self,
        world: &mut ActorWorld,
        frame: FrameReferenceId,
        name: &[u8],
        flags: u32,
    ) -> Result<Option<FrameReferenceId>, String>;
    fn set_child_enabled(
        &mut self,
        world: &mut ActorWorld,
        child: FrameReferenceId,
        enabled: u32,
    ) -> Result<(), String>;
    fn release_frame(
        &mut self,
        world: &mut ActorWorld,
        frame: FrameReferenceId,
    ) -> Result<(), String>;
}
/// Allocator for supported kind1/kind5 factory branches. Returning storage and
/// seed does not construct a frame, BODY or mission payload.
pub trait ActorAllocationBackend {
    fn allocate_actor(&mut self, bytes: u32) -> Result<Option<(u32, BaseActorSeed)>, String>;
    /// Kind5 requires actual larger storage and its untouched allocation seed.
    fn allocate_detector(
        &mut self,
        _bytes: u32,
    ) -> Result<Option<(u32, BaseActorSeed, DetectorSeed)>, String> {
        Err("detector allocation unavailable".into())
    }
}
/// Native scalar deleting-wrapper free dependency. Failed host frees retain
/// backing ownership in this backend; object destruction has already completed.
pub trait ActorDeallocationBackend {
    fn free_actor(&mut self, world: &ActorWorld, identity: u32) -> Result<(), String>;
}
/// One manager+0x90 association record (native stride 0x58). Unknown payload is
/// preserved; the flag-zero destructor branch only reads/clears `actor`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorAssociation {
    pub prefix: [u8; 80],
    pub actor: Option<ActorReferenceId>,
    pub tail: u32,
}
#[derive(Debug, Default)]
pub struct ActorManager {
    pub associations: Vec<ActorAssociation>,
    pub first: Vec<ActorReferenceId>,
    pub second: Vec<ActorReferenceId>,
}
impl ActorManager {
    /// Bounded flag-zero dependency of the base destructor. Flag-one resource
    /// callbacks and actor userdata cleanup are explicitly unsupported.
    pub fn remove_association(
        &mut self,
        actor: ActorReferenceId,
        flag: bool,
    ) -> Result<(), ActorWorldError> {
        if flag {
            return Err(ActorWorldError::UnsupportedManagerFlag);
        }
        if let Some(index) = self
            .associations
            .iter()
            .position(|r| r.actor == Some(actor))
        {
            self.associations[index].actor = None;
            self.associations.remove(index);
        }
        Ok(())
    }
}
/// Genuine Rust ownership of base actor storage plus separate native-comparable
/// source collections. This does not construct the mission's derived actors.
#[derive(Debug)]
pub struct ActorWorld {
    actors: Vec<OwnedActor>,
    static_actors: ActorCollection,
    primary: ActorCollection,
    active: ActorCollection,
    pub players: [Option<ActorReferenceId>; 2],
    pub actor_count: u32,
    pub attachment_counter: u32,
    pub manager: ActorManager,
}
impl ActorWorld {
    /// Supply actual admitted initial counter/backing state. Empty sources are
    /// owned here; caller-provided native backing tokens retain their allocator.
    pub fn new(
        actor_count: u32,
        attachment_counter: u32,
        static_backing: SourceBacking,
        primary_backing: SourceBacking,
        active_backing: SourceBacking,
    ) -> Result<Self, ActorWorldError> {
        Ok(Self {
            actors: Vec::new(),
            static_actors: ActorCollection::new(static_backing)?,
            primary: ActorCollection::new(primary_backing)?,
            active: ActorCollection::new(active_backing)?,
            players: [None; 2],
            actor_count,
            attachment_counter,
            manager: ActorManager::default(),
        })
    }
    fn collection(&self, source: ActorSource) -> &ActorCollection {
        match source {
            ActorSource::Static => &self.static_actors,
            ActorSource::Primary => &self.primary,
            ActorSource::Active => &self.active,
        }
    }
    fn collection_mut(&mut self, source: ActorSource) -> &mut ActorCollection {
        match source {
            ActorSource::Static => &mut self.static_actors,
            ActorSource::Primary => &mut self.primary,
            ActorSource::Active => &mut self.active,
        }
    }
    pub fn source(&self, source: ActorSource) -> &[Option<ActorReferenceId>] {
        &self.collection(source).values
    }
    pub fn backing(&self, source: ActorSource) -> SourceBacking {
        self.collection(source).backing
    }
    pub fn pending_backing(&self, source: ActorSource) -> Option<SourceBacking> {
        self.collection(source).pending
    }
    fn index(&self, actor: ActorReferenceId) -> Result<usize, ActorWorldError> {
        self.actors
            .iter()
            .position(|a| a.lifetime.reference() == actor && a.phase != ActorPhase::Dead)
            .ok_or(ActorWorldError::StaleActor(actor))
    }
    /// Read-only storage inspection, including retired/interrupted incarnations.
    /// This grants no live actor resolution and cannot mutate retired storage.
    pub fn inspect(&self, actor: ActorReferenceId) -> Option<(ActorPhase, &BaseActorState)> {
        self.actors
            .iter()
            .find(|a| a.lifetime.reference() == actor)
            .map(|a| (a.phase, &a.state))
    }
    pub fn state(&self, actor: ActorReferenceId) -> Result<&BaseActorState, ActorWorldError> {
        Ok(&self.actors[self.index(actor)?].state)
    }
    pub fn state_mut(
        &mut self,
        actor: ActorReferenceId,
    ) -> Result<&mut BaseActorState, ActorWorldError> {
        let i = self.index(actor)?;
        if self.actors[i].detector.is_some() {
            return Err(ActorWorldError::UnsupportedVtable);
        }
        Ok(&mut self.actors[i].state)
    }
    fn state_mut_inner(
        &mut self,
        actor: ActorReferenceId,
    ) -> Result<&mut BaseActorState, ActorWorldError> {
        let i = self.index(actor)?;
        Ok(&mut self.actors[i].state)
    }
    /// Mutable actual attachment for shared base callbacks. Derived vtable/kind
    /// identity remains protected; native Init rereads this frame between calls.
    pub fn actor_frame_mut(
        &mut self,
        actor: ActorReferenceId,
    ) -> Result<&mut Option<FrameReferenceId>, ActorWorldError> {
        Ok(&mut self.state_mut_inner(actor)?.frame)
    }
    pub fn phase(&self, actor: ActorReferenceId) -> Result<ActorPhase, ActorWorldError> {
        Ok(self.actors[self.index(actor)?].phase)
    }
    fn ready(&self, actor: ActorReferenceId) -> Result<usize, ActorWorldError> {
        let i = self.index(actor)?;
        if self.actors[i].phase != ActorPhase::Ready {
            return Err(ActorWorldError::Busy(actor));
        }
        Ok(i)
    }
    fn append<B: ActorCollectionBackend + ?Sized>(
        &mut self,
        source: ActorSource,
        actor: Option<ActorReferenceId>,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        let c = self.collection(source);
        if c.pending.is_some() {
            return Err(ActorWorldError::IncompleteGrowth(source));
        }
        let len = c.values.len();
        if len >= MAX_ACTOR_SOURCES {
            return Err(ActorWorldError::SourceLimit);
        }
        if len == c.backing.capacity {
            let capacity = len
                .checked_add(len.max(1))
                .filter(|n| *n <= MAX_ACTOR_SOURCES)
                .ok_or(ActorWorldError::SourceLimit)?;
            // Host storage is admitted before calling the native allocation dependency.
            self.collection_mut(source)
                .values
                .try_reserve(capacity - len)
                .map_err(|_| ActorWorldError::HostAllocation)?;
            let storage = backend
                .allocate_collection(self, source, (capacity * 4) as u32)
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "allocate_collection",
                    detail,
                })?;
            let new_backing = SourceBacking {
                storage: Some(storage),
                capacity,
            };
            self.collection_mut(source).pending = Some(new_backing);
            let old = self.collection(source).backing.storage;
            backend
                .free_collection(self, source, old)
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "free_collection",
                    detail,
                })?;
            let c = self.collection_mut(source);
            c.backing = new_backing;
            c.pending = None;
        }
        self.collection_mut(source).values.push(actor);
        Ok(())
    }
    /// Native static registration skips null and reverse-deduplicates pointers.
    pub fn add_actor_to_global<B: ActorCollectionBackend + ?Sized>(
        &mut self,
        actor: Option<ActorReferenceId>,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        let Some(actor) = actor else {
            return Ok(());
        };
        self.index(actor)?;
        if self
            .static_actors
            .values
            .iter()
            .rev()
            .any(|a| *a == Some(actor))
        {
            return Ok(());
        }
        self.append(ActorSource::Static, Some(actor), backend)
    }
    /// Static erasure fills the selected slot with the old final pointer.
    pub fn del_actor_from_global(&mut self, actor: Option<ActorReferenceId>) {
        if let Some(i) = self.static_actors.values.iter().rposition(|a| *a == actor) {
            self.static_actors.values.swap_remove(i);
        }
    }
    /// Context AddActor returns immediately for a primary duplicate, including
    /// an attempted false-to-true active promotion.
    pub fn add_actor<B: ActorCollectionBackend + ?Sized>(
        &mut self,
        actor: Option<ActorReferenceId>,
        active: bool,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        if let Some(a) = actor {
            self.index(a)?;
        }
        if self.primary.values.iter().rev().any(|a| *a == actor) {
            return Ok(());
        }
        self.append(ActorSource::Primary, actor, backend)?;
        if active && !self.active.values.iter().rev().any(|a| *a == actor) {
            self.append(ActorSource::Active, actor, backend)?;
        }
        Ok(())
    }
    /// Membership erasure preserves survivor order and never destroys an actor.
    /// The return predicate reports primary removal, including active-only cases.
    pub fn unload_actor(&mut self, actor: Option<ActorReferenceId>) -> bool {
        let primary = self.primary.values.iter().rposition(|a| *a == actor);
        if let Some(i) = primary {
            self.primary.values.remove(i);
        }
        if let Some(i) = self.active.values.iter().rposition(|a| *a == actor) {
            self.active.values.remove(i);
        }
        primary.is_some()
    }
    /// The constructor stores only evidenced fields, then increments the global
    /// count before static registration. Interrupted registration retains actor
    /// storage and its identity in Constructing phase, never a usable lookup actor.
    pub fn construct_base<B: ActorCollectionBackend + ?Sized>(
        &mut self,
        identity: u32,
        seed: BaseActorSeed,
        backend: &mut B,
    ) -> Result<ActorReferenceId, ActorCreationError> {
        let fail = |source| ActorCreationError {
            actor: None,
            source,
        };
        if identity == 0 {
            return Err(fail(ActorWorldError::NullIdentity));
        }
        if self
            .actors
            .iter()
            .any(|a| a.phase != ActorPhase::Dead && a.lifetime.reference().identity() == identity)
        {
            return Err(fail(ActorWorldError::DuplicateIdentity));
        }
        if self.actors.len() >= MAX_ACTOR_SOURCES {
            return Err(fail(ActorWorldError::SourceLimit));
        }
        self.actors
            .try_reserve(1)
            .map_err(|_| fail(ActorWorldError::HostAllocation))?;
        let lifetime = ActorLifetime::for_existing(identity);
        let actor = lifetime.reference();
        self.actors.push(OwnedActor {
            lifetime,
            state: BaseActorState::construct(seed),
            phase: ActorPhase::Constructing,
            detector: None,
        });
        self.actor_count = self.actor_count.wrapping_add(1);
        self.add_actor_to_global(Some(actor), backend)
            .map_err(|source| ActorCreationError {
                actor: Some(actor),
                source,
            })?;
        let i = self.index(actor).expect("constructor owns actor");
        self.actors[i].phase = ActorPhase::Ready;
        Ok(actor)
    }
    /// Base registration completes before the embedded source program and
    /// detector vtables are installed. No compiler/GameInit is invoked.
    pub fn construct_detector<B: ActorCollectionBackend + ?Sized>(
        &mut self,
        identity: u32,
        base: BaseActorSeed,
        mut seed: DetectorSeed,
        backend: &mut B,
    ) -> Result<ActorReferenceId, ActorCreationError> {
        let program_address = identity.checked_add(0x70).ok_or(ActorCreationError {
            actor: None,
            source: ActorWorldError::InvalidBacking,
        })?;
        identity.checked_add(0x127).ok_or(ActorCreationError {
            actor: None,
            source: ActorWorldError::InvalidBacking,
        })?;
        seed.program.address_high_byte = (program_address >> 24) as u8;
        let actor = self.construct_base(identity, base, backend)?;
        let i = self.index(actor).expect("constructor owns actor");
        self.actors[i].detector = Some(DetectorState {
            program: Some(CProgram::construct(seed.program)),
            bytes_d8_ef: seed.bytes_d8_ef,
            word_f0: 0,
            word_f4: seed.word_f4,
            words_f8_100: [0; 3],
            byte_104: 0,
            bytes_105_107: seed.bytes_105_107,
            words_108_114: [0; 4],
            bytes_118_127: seed.bytes_118_127,
        });
        self.actors[i]
            .detector
            .as_mut()
            .unwrap()
            .program
            .as_mut()
            .unwrap()
            .set_detector_vtable();
        self.actors[i].state.vtable = ActorVtable::Detector;
        self.actors[i].state.word_10 = 5;
        Ok(actor)
    }
    pub fn detector(&self, actor: ActorReferenceId) -> Result<&DetectorState, ActorWorldError> {
        self.actors[self.index(actor)?]
            .detector
            .as_ref()
            .ok_or(ActorWorldError::NotDetector)
    }
    /// Read-only source ownership inspection, including retired/interrupted actors.
    pub fn inspect_detector(&self, actor: ActorReferenceId) -> Option<&DetectorState> {
        self.actors
            .iter()
            .find(|a| a.lifetime.reference() == actor)?
            .detector
            .as_ref()
    }
    /// Supported original factory branches: kind1 allocates 0x70, kind5 0x128.
    /// Other derived kinds remain explicit dependencies.
    pub fn create_actor<B: ActorAllocationBackend + ActorCollectionBackend + ?Sized>(
        &mut self,
        kind: u32,
        backend: &mut B,
    ) -> Result<Option<ActorReferenceId>, ActorCreationError> {
        if !(1..=38).contains(&kind) {
            return Ok(None);
        }
        if kind == 5 {
            let allocation =
                backend
                    .allocate_detector(0x128)
                    .map_err(|detail| ActorCreationError {
                        actor: None,
                        source: ActorWorldError::Effect {
                            stage: "allocate_detector",
                            detail,
                        },
                    })?;
            return allocation
                .map(|(identity, base, seed)| {
                    self.construct_detector(identity, base, seed, backend)
                })
                .transpose();
        }
        if kind != 1 {
            return Err(ActorCreationError {
                actor: None,
                source: ActorWorldError::UnsupportedKind(kind),
            });
        }
        let allocation = backend
            .allocate_actor(0x70)
            .map_err(|detail| ActorCreationError {
                actor: None,
                source: ActorWorldError::Effect {
                    stage: "allocate_actor",
                    detail,
                },
            })?;
        let Some((identity, seed)) = allocation else {
            return Ok(None);
        };
        self.construct_base(identity, seed, backend).map(Some)
    }
    pub fn init<B: ActorFrameBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        frame: Option<FrameReferenceId>,
        backend: &mut B,
    ) -> Result<bool, ActorWorldError> {
        let i = self.ready(actor)?;
        let Some(frame) = frame else {
            return Ok(false);
        };
        self.actors[i].phase = ActorPhase::Initializing;
        let mut result = self.init_inner(actor, frame, backend);
        let i = self.index(actor)?;
        if matches!(result, Ok(true)) {
            if let Some(detector) = &mut self.actors[i].detector {
                detector.bytes_d8_ef[..4].fill(0);
                if let Err(error) = detector
                    .program
                    .as_mut()
                    .expect("ready detector program")
                    .set_owner(Some(actor))
                {
                    result = Err(ActorWorldError::Program(error));
                }
            }
        }
        self.actors[i].phase = if result.is_err() {
            ActorPhase::InitializationInterrupted
        } else {
            ActorPhase::Ready
        };
        result
    }
    fn init_inner<B: ActorFrameBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        frame: FrameReferenceId,
        backend: &mut B,
    ) -> Result<bool, ActorWorldError> {
        let data = backend
            .actor_data(self, frame)
            .map_err(|detail| ActorWorldError::Effect {
                stage: "frame_actor_data",
                detail,
            })?;
        if matches!(data, FrameActorData::Present(Some(owner)) if owner != actor) {
            return Ok(false);
        }
        self.state_mut_inner(actor)?.frame = Some(frame);
        backend
            .clear_actor_data(self, frame)
            .map_err(|detail| ActorWorldError::Effect {
                stage: "clear_frame_actor_data",
                detail,
            })?;
        self.attachment_counter = self.attachment_counter.wrapping_add(1);
        let counter = self.attachment_counter;
        let state = self.state_mut_inner(actor)?;
        state.actor_id = counter;
        state.release_frame = 0;
        // The first null check guards the entire pair. The second native call
        // rereads actor+0x68 without a second null guard after callback return.
        if let Some(frame) = self.state(actor)?.frame {
            self.init_target_child(frame, b"targetE", backend)?;
            let frame = self
                .state(actor)?
                .frame
                .ok_or(ActorWorldError::MissingAttachedFrame)?;
            self.init_target_child(frame, b"targetN", backend)?;
        }
        self.state_mut_inner(actor)?.state_1c = 0;
        Ok(true)
    }
    fn init_target_child<B: ActorFrameBackend + ?Sized>(
        &mut self,
        frame: FrameReferenceId,
        name: &[u8],
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        let child = backend
            .find_child(self, frame, name, 0x40)
            .map_err(|detail| ActorWorldError::Effect {
                stage: "find_target_child",
                detail,
            })?;
        if let Some(child) = child {
            backend
                .set_child_enabled(self, child, 0)
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "disable_target_child",
                    detail,
                })?;
        }
        Ok(())
    }
    fn remove_from_manager(&mut self, actor: ActorReferenceId) -> Result<bool, ActorWorldError> {
        self.manager.remove_association(actor, false)?;
        if let Some(i) = self.manager.first.iter().position(|a| *a == actor) {
            self.state_mut_inner(actor)?.flags &= !0x10;
            self.manager.first.remove(i);
            return Ok(true);
        }
        if let Some(i) = self.manager.second.iter().position(|a| *a == actor) {
            self.manager.second.remove(i);
            return Ok(true);
        }
        Ok(false)
    }
    /// Actual base destruction: decrement, manager flag-zero removal, conditional
    /// frame virtual0, static erasure. Context/player membership is independent.
    /// A failed frame release retains an explicitly unusable owned actor and
    /// cannot be retried by repeating destruction and decrementing again.
    pub fn destroy_base<B: ActorFrameBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].detector.is_some() {
            return Err(ActorWorldError::UnsupportedVtable);
        }
        self.destroy_base_inner(actor, backend)
    }
    fn destroy_base_inner<B: ActorFrameBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        let i = self.index(actor)?;
        self.actors[i].phase = ActorPhase::Destroying;
        self.actors[i].state.vtable = ActorVtable::Base;
        self.actor_count = self.actor_count.wrapping_sub(1);
        self.remove_from_manager(actor)?;
        let state = self.state(actor)?;
        if state.release_frame != 0 {
            if let Some(frame) = state.frame {
                if let Err(detail) = backend.release_frame(self, frame) {
                    let i = self.index(actor)?;
                    self.actors[i].phase = ActorPhase::DestructionInterrupted;
                    return Err(ActorWorldError::Effect {
                        stage: "release_actor_frame",
                        detail,
                    });
                }
            }
        }
        self.del_actor_from_global(Some(actor));
        let i = self.index(actor)?;
        self.actors[i].phase = ActorPhase::Dead;
        Ok(())
    }
    /// Scalar deleting wrapper: base destruction precedes optional memory free;
    /// only argument bit0 controls the free. Death does not depend on free success.
    pub fn delete_base<B: ActorFrameBackend + ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        argument: u32,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        self.destroy_base(actor, backend)?;
        if argument & 1 != 0 {
            backend
                .free_actor(self, actor.identity())
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "free_actor",
                    detail,
                })?;
        }
        Ok(())
    }
    /// Base virtual Release decrements the native 16-bit field with wrapping;
    /// reaching zero calls the deleting wrapper with argument1. No unknown
    /// derived virtual deleting destructor is supplied by this base-only owner.
    pub fn release<B: ActorFrameBackend + ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        backend: &mut B,
    ) -> Result<u16, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].state.vtable != ActorVtable::Base || self.actors[i].detector.is_some() {
            return Err(ActorWorldError::UnsupportedVtable);
        }
        let count = self.actors[i].state.references.wrapping_sub(1);
        self.actors[i].state.references = count;
        if count == 0 {
            self.delete_base(actor, 1, backend)?;
        }
        Ok(count)
    }
    /// Detector virtual LoadData: the detector's version and raw d8 field precede
    /// embedded source disposal/reads. Source callbacks borrow only the program;
    /// they cannot mutate this exclusively borrowed world during replacement.
    pub fn load_detector_data<R: ProgramReader + ?Sized, P: ProgramBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        reader: &mut R,
        replacements: &[ProgramReplacement],
        mission_path: Option<&[u8]>,
        frames: &dyn FrameLookupBackend,
        backend: &mut P,
    ) -> Result<(), ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].detector.is_none() {
            return Err(ActorWorldError::NotDetector);
        }
        self.actors[i].phase = ActorPhase::Loading;
        let read = reader.read_exact(&mut [0]).and_then(|()| {
            reader.read_exact(&mut self.actors[i].detector.as_mut().unwrap().bytes_d8_ef[..4])
        });
        if let Err(detail) = read {
            self.actors[i].phase = ActorPhase::LoadingInterrupted;
            return Err(ActorWorldError::Effect {
                stage: "detector_read",
                detail,
            });
        }
        let mut program = self.actors[i]
            .detector
            .as_mut()
            .unwrap()
            .program
            .take()
            .unwrap();
        let mut context = ActorProgramContext {
            world: self,
            frames,
            mission_path,
        };
        let result = program.load_source_code(reader, replacements, &mut context, backend);
        self.actors[i].detector.as_mut().unwrap().program = Some(program);
        self.actors[i].phase = if result.is_ok() {
            ActorPhase::Ready
        } else {
            ActorPhase::LoadingInterrupted
        };
        result.map_err(ActorWorldError::Program)
    }
    /// Source-only detector destruction: embedded program before the genuine
    /// base destructor. A source free failure prevents all base destruction.
    pub fn destroy_detector<F: ActorFrameBackend + ?Sized, P: ProgramBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        frames: &mut F,
        backend: &mut P,
    ) -> Result<(), ActorWorldError> {
        let i = self.index(actor)?;
        if !matches!(
            self.actors[i].phase,
            ActorPhase::Ready
                | ActorPhase::LoadingInterrupted
                | ActorPhase::InitializationInterrupted
        ) {
            return Err(ActorWorldError::Busy(actor));
        }
        if self.actors[i].detector.is_none() {
            return Err(ActorWorldError::NotDetector);
        }
        self.actors[i].phase = ActorPhase::Destroying;
        self.actors[i].state.vtable = ActorVtable::Detector;
        let result = self.actors[i]
            .detector
            .as_mut()
            .unwrap()
            .program
            .as_mut()
            .unwrap()
            .destruct(backend);
        if let Err(error) = result {
            self.actors[i].phase = ActorPhase::DestructionInterrupted;
            return Err(ActorWorldError::Program(error));
        }
        self.destroy_base_inner(actor, frames)
    }
    pub fn delete_detector<
        F: ActorFrameBackend + ?Sized,
        P: ProgramBackend + ActorDeallocationBackend + ?Sized,
    >(
        &mut self,
        actor: ActorReferenceId,
        argument: u32,
        frames: &mut F,
        backend: &mut P,
    ) -> Result<(), ActorWorldError> {
        self.destroy_detector(actor, frames, backend)?;
        if argument & 1 != 0 {
            backend
                .free_actor(self, actor.identity())
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "free_detector",
                    detail,
                })?;
        }
        Ok(())
    }
    /// Shared 16-bit Release dispatched to the detector deleting wrapper.
    pub fn release_detector<
        F: ActorFrameBackend + ?Sized,
        P: ProgramBackend + ActorDeallocationBackend + ?Sized,
    >(
        &mut self,
        actor: ActorReferenceId,
        frames: &mut F,
        backend: &mut P,
    ) -> Result<u16, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].detector.is_none() || self.actors[i].state.vtable != ActorVtable::Detector
        {
            return Err(ActorWorldError::NotDetector);
        }
        let count = self.actors[i].state.references.wrapping_sub(1);
        self.actors[i].state.references = count;
        if count == 0 {
            self.delete_detector(actor, 1, frames, backend)?;
        }
        Ok(count)
    }
}
/// Explicit diagnostic effect with access to the same live owner/collections.
/// Subsequent lookup reads observe callback mutations, including a changed owner.
pub trait ActorWorldDiagnosticBackend {
    fn message_beep(
        &mut self,
        world: &mut ActorWorld,
        owner: &mut Option<ActorReferenceId>,
        argument: i32,
    ) -> Result<(), LookupError>;
}
pub struct ActorWorldLookup<'a> {
    pub world: &'a mut ActorWorld,
    pub owner: Option<ActorReferenceId>,
    pub frames: &'a dyn FrameLookupBackend,
    pub scene: Option<SceneRoots>,
    pub diagnostic: &'a mut dyn ActorWorldDiagnosticBackend,
    pub diagnostic_default_c_locale: bool,
}
impl LookupWorldBackend for ActorWorldLookup<'_> {
    fn diagnostic_default_c_locale(&self) -> bool {
        self.diagnostic_default_c_locale
    }
    fn owner(&self) -> Result<Option<ActorReferenceId>, LookupError> {
        Ok(self.owner)
    }
    fn actor_frame(
        &self,
        actor: ActorReferenceId,
    ) -> Result<Option<FrameReferenceId>, LookupError> {
        let i = self
            .world
            .index(actor)
            .map_err(|_| LookupError::StaleActor(actor))?;
        if self.world.actors[i].phase != ActorPhase::Ready {
            return Err(LookupError::Backend("actor lifecycle is incomplete".into()));
        }
        Ok(self.world.actors[i].state.frame)
    }
    fn primary(&self) -> Result<&[Option<ActorReferenceId>], LookupError> {
        Ok(self.world.source(ActorSource::Primary))
    }
    fn players(&self) -> Result<[Option<ActorReferenceId>; 2], LookupError> {
        Ok(self.world.players)
    }
    fn secondary(&self) -> Result<&[Option<ActorReferenceId>], LookupError> {
        Ok(self.world.source(ActorSource::Static))
    }
    fn frames(&self) -> Result<&dyn FrameLookupBackend, LookupError> {
        Ok(self.frames)
    }
    fn scene(&self) -> Result<SceneRoots, LookupError> {
        self.scene.ok_or(LookupError::MissingScene)
    }
    fn message_beep(&mut self, argument: i32) -> Result<(), LookupError> {
        self.diagnostic
            .message_beep(self.world, &mut self.owner, argument)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_lookup::{FrameLifetime, FrameLookupError, FrameView};
    use crate::script_lookup;
    use crate::script_scheduler::ControlState;

    fn control() -> ControlState {
        use crate::named_script::ResetState;
        use crate::script_scheduler::{
            Program, ProgramBackend, SchedulerBackend, ScriptEffects, StartupCommand,
        };
        struct NoEffects;
        impl ScriptEffects for NoEffects {
            type Error = std::convert::Infallible;
            fn execute(
                &mut self,
                _: StartupCommand,
                _: &mut ControlState,
                _: &mut ResetState,
                _: i32,
            ) -> Result<(), Self::Error> {
                panic!("declaration must not execute effects")
            }
        }
        let mut control = ControlState::new(0, 0x3f).unwrap();
        let program = Program::from_source("dim_act 1").unwrap();
        let mut state = ResetState {
            program_counter: 0,
            short_stack_begin: 0,
            short_stack_end: 0,
            field_44: 0,
            continuation: 1,
        };
        ProgramBackend {
            program: &program,
            control: &mut control,
            effects: &mut NoEffects,
        }
        .dispatch(&mut state, 0)
        .unwrap();
        control
    }
    fn seed() -> BaseActorSeed {
        BaseActorSeed {
            word_0e: 0xabcd,
            actor_id: 0x12345678,
            state_1c: 0x87654321,
            bytes_21_5f: [0x21; 63],
            bytes_64_65: [0x65; 2],
            byte_67: 0x67,
            release_frame: 0x6c,
            bytes_6d_6f: [0x6f; 3],
        }
    }
    fn backing(capacity: usize) -> SourceBacking {
        SourceBacking {
            storage: (capacity != 0).then_some(ActorStorageId(10)),
            capacity,
        }
    }
    fn world() -> ActorWorld {
        ActorWorld::new(0, 0, backing(8), backing(8), backing(8)).unwrap()
    }
    #[derive(Debug, PartialEq, Eq)]
    enum Event {
        Allocate(ActorSource, u32, usize, u32),
        Free(ActorSource, Option<ActorStorageId>, usize),
        AllocateActor(u32),
        Data(FrameReferenceId),
        Clear(FrameReferenceId, Option<FrameReferenceId>),
        Find(FrameReferenceId, Vec<u8>, u32, u32, u32),
        Enable(FrameReferenceId, u32, u32),
        ReleaseFrame(FrameReferenceId, u32, usize),
        FreeActor(u32, u32, usize),
    }
    #[derive(Default)]
    struct Effects {
        events: Vec<Event>,
        fail: Option<&'static str>,
        fail_source: Option<ActorSource>,
        actor: Option<ActorReferenceId>,
        userdata: Option<FrameActorData>,
        child: Option<FrameReferenceId>,
        replace_after_e: Option<Option<FrameReferenceId>>,
        allocated: u32,
    }
    impl ActorCollectionBackend for Effects {
        fn allocate_collection(
            &mut self,
            world: &ActorWorld,
            source: ActorSource,
            bytes: u32,
        ) -> Result<ActorStorageId, String> {
            self.events.push(Event::Allocate(
                source,
                bytes,
                world.source(ActorSource::Primary).len(),
                world.actor_count,
            ));
            if self.fail == Some("allocate") && self.fail_source.is_none_or(|s| s == source) {
                return Err("unavailable".into());
            }
            self.allocated += 1;
            Ok(ActorStorageId(100 + self.allocated))
        }
        fn free_collection(
            &mut self,
            world: &ActorWorld,
            source: ActorSource,
            old: Option<ActorStorageId>,
        ) -> Result<(), String> {
            self.events.push(Event::Free(
                source,
                old,
                world.source(ActorSource::Primary).len(),
            ));
            if self.fail == Some("free_collection") {
                return Err("unavailable".into());
            }
            Ok(())
        }
    }
    impl ActorAllocationBackend for Effects {
        fn allocate_actor(&mut self, bytes: u32) -> Result<Option<(u32, BaseActorSeed)>, String> {
            self.events.push(Event::AllocateActor(bytes));
            if self.fail == Some("allocate_actor") {
                return Err("unavailable".into());
            }
            if self.fail == Some("null_actor") {
                return Ok(None);
            }
            Ok(Some((42, seed())))
        }
    }
    impl ActorFrameBackend for Effects {
        fn actor_data(
            &mut self,
            _: &mut ActorWorld,
            frame: FrameReferenceId,
        ) -> Result<FrameActorData, String> {
            self.events.push(Event::Data(frame));
            Ok(self.userdata.unwrap_or(FrameActorData::Null))
        }
        fn clear_actor_data(
            &mut self,
            world: &mut ActorWorld,
            frame: FrameReferenceId,
        ) -> Result<(), String> {
            self.events.push(Event::Clear(
                frame,
                world.state(self.actor.unwrap()).unwrap().frame,
            ));
            self.userdata = Some(FrameActorData::Null);
            Ok(())
        }
        fn find_child(
            &mut self,
            world: &mut ActorWorld,
            frame: FrameReferenceId,
            name: &[u8],
            flags: u32,
        ) -> Result<Option<FrameReferenceId>, String> {
            let actor = self.actor.unwrap();
            self.events.push(Event::Find(
                frame,
                name.to_vec(),
                flags,
                world.state(actor).unwrap().state_1c,
                world.state(actor).unwrap().actor_id,
            ));
            if name == b"targetE" {
                if let Some(replacement) = self.replace_after_e {
                    *world.actor_frame_mut(actor).unwrap() = replacement;
                }
            }
            if self.fail == Some("find") {
                return Err("unavailable".into());
            }
            Ok(self.child)
        }
        fn set_child_enabled(
            &mut self,
            world: &mut ActorWorld,
            child: FrameReferenceId,
            enabled: u32,
        ) -> Result<(), String> {
            self.events.push(Event::Enable(
                child,
                enabled,
                world.state(self.actor.unwrap()).unwrap().state_1c,
            ));
            if self.fail == Some("enable") {
                return Err("unavailable".into());
            }
            Ok(())
        }
        fn release_frame(
            &mut self,
            world: &mut ActorWorld,
            frame: FrameReferenceId,
        ) -> Result<(), String> {
            self.events.push(Event::ReleaseFrame(
                frame,
                world.actor_count,
                world.source(ActorSource::Static).len(),
            ));
            if self.fail == Some("release") {
                return Err("unavailable".into());
            }
            Ok(())
        }
    }
    impl ActorDeallocationBackend for Effects {
        fn free_actor(&mut self, world: &ActorWorld, identity: u32) -> Result<(), String> {
            self.events.push(Event::FreeActor(
                identity,
                world.actor_count,
                world.source(ActorSource::Static).len(),
            ));
            if self.fail == Some("free_actor") {
                return Err("unavailable".into());
            }
            Ok(())
        }
    }
    fn actor(world: &mut ActorWorld, effects: &mut Effects, id: u32) -> ActorReferenceId {
        world.construct_base(id, seed(), effects).unwrap()
    }
    #[test]
    fn constructor_preserves_every_unwritten_seed_field_and_wraps_count() {
        let mut w = world();
        let mut e = Effects::default();
        w.actor_count = u32::MAX;
        let a = actor(&mut w, &mut e, 1);
        let s = w.state(a).unwrap();
        assert_eq!(s.vtable, ActorVtable::Base);
        assert_eq!(
            (s.flags, s.user_data, s.references, s.word_10, s.word_14),
            (0, 0, 1, 1, 1)
        );
        assert_eq!((s.byte_20, s.word_60, s.byte_66, s.frame), (0, 0, 0, None));
        assert_eq!(
            BaseActorSeed {
                word_0e: s.word_0e,
                actor_id: s.actor_id,
                state_1c: s.state_1c,
                bytes_21_5f: s.bytes_21_5f,
                bytes_64_65: s.bytes_64_65,
                byte_67: s.byte_67,
                release_frame: s.release_frame,
                bytes_6d_6f: s.bytes_6d_6f
            },
            seed()
        );
        assert_eq!(w.actor_count, 0);
        assert_eq!(w.source(ActorSource::Static), &[Some(a)]);
    }
    #[test]
    fn growth_effects_observe_old_headers_and_primary_commits_before_active() {
        let mut w = ActorWorld::new(0, 0, backing(0), backing(0), backing(0)).unwrap();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        assert_eq!(
            e.events,
            [
                Event::Allocate(ActorSource::Static, 4, 0, 1),
                Event::Free(ActorSource::Static, None, 0)
            ]
        );
        e.events.clear();
        w.add_actor(Some(a), true, &mut e).unwrap();
        assert_eq!(
            e.events,
            [
                Event::Allocate(ActorSource::Primary, 4, 0, 1),
                Event::Free(ActorSource::Primary, None, 0),
                Event::Allocate(ActorSource::Active, 4, 1, 1),
                Event::Free(ActorSource::Active, None, 1)
            ]
        );
        assert_eq!(w.backing(ActorSource::Active).capacity, 1);
        let b = actor(&mut w, &mut e, 2);
        let c = actor(&mut w, &mut e, 3);
        assert_eq!(w.backing(ActorSource::Static).capacity, 4);
        assert_ne!(b, c);
    }
    #[test]
    fn active_allocation_failure_preserves_committed_primary() {
        let mut w = ActorWorld::new(0, 0, backing(1), backing(1), backing(0)).unwrap();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        e.fail = Some("allocate");
        e.fail_source = Some(ActorSource::Active);
        assert!(w.add_actor(Some(a), true, &mut e).is_err());
        assert_eq!(w.source(ActorSource::Primary), &[Some(a)]);
        assert!(w.source(ActorSource::Active).is_empty());
        e.fail = None;
        w.add_actor(Some(a), true, &mut e).unwrap();
        assert!(w.source(ActorSource::Active).is_empty());
    }
    #[test]
    fn failed_free_retains_uncommitted_backing_and_interrupted_constructor() {
        let mut w = ActorWorld::new(0, 0, backing(0), backing(0), backing(0)).unwrap();
        let mut e = Effects {
            fail: Some("free_collection"),
            ..Effects::default()
        };
        let error = w.construct_base(1, seed(), &mut e).unwrap_err();
        let a = error.actor.unwrap();
        assert_eq!(w.phase(a), Ok(ActorPhase::Constructing));
        assert_eq!(w.actor_count, 1);
        assert!(w.source(ActorSource::Static).is_empty());
        assert_eq!(w.backing(ActorSource::Static), backing(0));
        assert_eq!(w.pending_backing(ActorSource::Static).unwrap().capacity, 1);
        assert_eq!(
            w.add_actor_to_global(Some(a), &mut e),
            Err(ActorWorldError::IncompleteGrowth(ActorSource::Static))
        );
        assert!(matches!(
            w.destroy_base(a, &mut e),
            Err(ActorWorldError::Busy(_))
        ));
    }
    #[test]
    fn duplicate_primary_does_not_promote_active_and_sources_erase_differently() {
        let mut w = world();
        let mut e = Effects::default();
        let aa: Vec<_> = (1..=4).map(|id| actor(&mut w, &mut e, id)).collect();
        for a in &aa {
            w.add_actor(Some(*a), false, &mut e).unwrap();
        }
        w.add_actor(Some(aa[0]), true, &mut e).unwrap();
        assert!(w.source(ActorSource::Active).is_empty());
        w.del_actor_from_global(Some(aa[1]));
        assert!(w.unload_actor(Some(aa[1])));
        assert_eq!(
            w.source(ActorSource::Static),
            &[Some(aa[0]), Some(aa[3]), Some(aa[2])]
        );
        assert_eq!(
            w.source(ActorSource::Primary),
            &[Some(aa[0]), Some(aa[2]), Some(aa[3])]
        );
    }
    #[test]
    fn null_static_add_is_skipped_and_active_only_unload_returns_false() {
        let mut w = world();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        w.add_actor_to_global(None, &mut e).unwrap();
        w.active.values.extend([Some(a), None, Some(a)]);
        assert!(!w.unload_actor(Some(a)));
        assert_eq!(w.source(ActorSource::Active), &[Some(a), None]);
        w.add_actor(None, false, &mut e).unwrap();
        assert_eq!(w.source(ActorSource::Primary), &[None]);
        assert!(w.unload_actor(None));
        assert_eq!(w.source(ActorSource::Active), &[Some(a)]);
    }
    #[test]
    fn init_rejects_null_or_conflicting_owner_without_state_changes() {
        let mut w = world();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        let b = actor(&mut w, &mut e, 2);
        e.actor = Some(a);
        let frame = FrameLifetime::for_existing(1);
        let old = w.state(a).unwrap().clone();
        assert_eq!(w.init(a, None, &mut e), Ok(false));
        assert!(e.events.is_empty());
        e.userdata = Some(FrameActorData::Present(Some(b)));
        assert_eq!(w.init(a, Some(frame.reference()), &mut e), Ok(false));
        assert_eq!(w.state(a).unwrap(), &old);
        assert_eq!(w.attachment_counter, 0);
    }
    #[test]
    fn init_accepts_present_null_owner_and_reloads_frame_between_child_callbacks() {
        let mut w = world();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        let f = FrameLifetime::for_existing(1);
        let replacement = FrameLifetime::for_existing(2);
        let child = FrameLifetime::for_existing(3);
        e.actor = Some(a);
        e.userdata = Some(FrameActorData::Present(None));
        e.child = Some(child.reference());
        e.replace_after_e = Some(Some(replacement.reference()));
        w.attachment_counter = u32::MAX;
        assert_eq!(w.init(a, Some(f.reference()), &mut e), Ok(true));
        assert_eq!(
            e.events,
            [
                Event::Data(f.reference()),
                Event::Clear(f.reference(), Some(f.reference())),
                Event::Find(f.reference(), b"targetE".to_vec(), 0x40, seed().state_1c, 0),
                Event::Enable(child.reference(), 0, seed().state_1c),
                Event::Find(
                    replacement.reference(),
                    b"targetN".to_vec(),
                    0x40,
                    seed().state_1c,
                    0
                ),
                Event::Enable(child.reference(), 0, seed().state_1c)
            ]
        );
        let s = w.state(a).unwrap();
        assert_eq!((s.actor_id, s.state_1c, s.release_frame), (0, 0, 0));
        assert_eq!(s.frame, Some(replacement.reference()));
    }
    #[test]
    fn callback_failure_preserves_attachment_prefix_and_no_completed_lifecycle() {
        let mut w = world();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        let f = FrameLifetime::for_existing(1);
        e.actor = Some(a);
        e.child = Some(f.reference());
        e.fail = Some("enable");
        assert!(w.init(a, Some(f.reference()), &mut e).is_err());
        let s = w.state(a).unwrap();
        assert_eq!(
            (s.frame, s.actor_id, s.release_frame, s.state_1c),
            (Some(f.reference()), 1, 0, seed().state_1c)
        );
        assert_eq!(w.phase(a), Ok(ActorPhase::InitializationInterrupted));
        assert_eq!(e.events.len(), 4);
    }
    #[test]
    fn null_frame_after_first_callback_is_explicit_error_before_second_native_call() {
        let mut w = world();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        let f = FrameLifetime::for_existing(1);
        e.actor = Some(a);
        e.replace_after_e = Some(None);
        assert_eq!(
            w.init(a, Some(f.reference()), &mut e),
            Err(ActorWorldError::MissingAttachedFrame)
        );
        assert_eq!(w.state(a).unwrap().state_1c, seed().state_1c);
        assert_eq!(w.phase(a), Ok(ActorPhase::InitializationInterrupted));
    }
    #[test]
    fn destructor_removes_first_manager_matches_in_order_and_keeps_other_sources() {
        let mut w = world();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        let b = actor(&mut w, &mut e, 2);
        w.add_actor(Some(a), true, &mut e).unwrap();
        w.players[0] = Some(a);
        w.state_mut(a).unwrap().flags = 0x35;
        w.manager.first = vec![b, a, b];
        w.manager.second = vec![a, b];
        w.manager.associations = vec![
            ActorAssociation {
                prefix: [1; 80],
                actor: Some(b),
                tail: 1,
            },
            ActorAssociation {
                prefix: [2; 80],
                actor: Some(a),
                tail: 2,
            },
            ActorAssociation {
                prefix: [3; 80],
                actor: Some(a),
                tail: 3,
            },
        ];
        w.destroy_base(a, &mut e).unwrap();
        assert_eq!(w.manager.first, [b, b]);
        assert_eq!(w.manager.second, [a, b]);
        assert_eq!(
            w.manager
                .associations
                .iter()
                .map(|r| r.tail)
                .collect::<Vec<_>>(),
            [1, 3]
        );
        assert_eq!(w.manager.associations[1].prefix, [3; 80]);
        assert_eq!(w.source(ActorSource::Primary), &[Some(a)]);
        assert_eq!(w.players[0], Some(a));
        assert_eq!(w.source(ActorSource::Static), &[Some(b)]);
        assert!(w.state(a).is_err());
        assert!(e.events.is_empty());
        assert_eq!(
            w.manager.remove_association(b, true),
            Err(ActorWorldError::UnsupportedManagerFlag)
        );
    }
    #[test]
    fn failed_frame_release_preserves_destruction_prefix_and_prevents_double_decrement() {
        let mut w = world();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        let f = FrameLifetime::for_existing(1);
        w.state_mut(a).unwrap().frame = Some(f.reference());
        w.state_mut(a).unwrap().flags = 0x31;
        w.state_mut(a).unwrap().vtable = ActorVtable::Opaque(8);
        w.manager.first.push(a);
        w.actor_count = 0;
        e.fail = Some("release");
        assert!(w.destroy_base(a, &mut e).is_err());
        assert_eq!(e.events, [Event::ReleaseFrame(f.reference(), u32::MAX, 1)]);
        assert_eq!(w.state(a).unwrap().flags, 0x21);
        assert_eq!(w.state(a).unwrap().vtable, ActorVtable::Base);
        assert_eq!(w.phase(a), Ok(ActorPhase::DestructionInterrupted));
        assert!(w.manager.first.is_empty());
        assert_eq!(w.source(ActorSource::Static), &[Some(a)]);
        assert_eq!(w.destroy_base(a, &mut e), Err(ActorWorldError::Busy(a)));
        assert_eq!(w.actor_count, u32::MAX);
    }
    #[test]
    fn base_attachment_clears_conditional_release_and_deleting_wrapper_uses_bit_zero() {
        for flag in [0, 1, 2, 3] {
            let mut w = world();
            let mut e = Effects::default();
            let a = actor(&mut w, &mut e, 1);
            let f = FrameLifetime::for_existing(1);
            e.actor = Some(a);
            w.init(a, Some(f.reference()), &mut e).unwrap();
            e.events.clear();
            w.delete_base(a, flag, &mut e).unwrap();
            assert_eq!(
                e.events,
                if flag & 1 != 0 {
                    vec![Event::FreeActor(1, 0, 0)]
                } else {
                    vec![]
                }
            );
        }
    }
    #[test]
    fn release_word_wraps_and_free_failure_does_not_revive_destroyed_actor() {
        let mut w = world();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        w.state_mut(a).unwrap().references = 0;
        w.release(a, &mut e).unwrap();
        assert_eq!(w.state(a).unwrap().references, u16::MAX);
        assert_eq!(w.actor_count, 1);
        w.state_mut(a).unwrap().references = 1;
        e.fail = Some("free_actor");
        assert!(w.release(a, &mut e).is_err());
        assert!(w.state(a).is_err());
        assert_eq!(w.actor_count, 0);
        assert!(w.source(ActorSource::Static).is_empty());
    }
    struct Frames(Vec<(FrameLifetime, Vec<u8>)>);
    impl FrameLookupBackend for Frames {
        fn default_c_locale(&self) -> bool {
            true
        }
        fn frame(&self, f: FrameReferenceId) -> Result<FrameView<'_>, FrameLookupError> {
            let (_, name) = self
                .0
                .iter()
                .find(|(lease, _)| lease.reference() == f)
                .ok_or(FrameLookupError::StaleFrame(f))?;
            Ok(FrameView {
                name: Some(name),
                child: None,
                next: None,
            })
        }
    }
    #[derive(Default)]
    struct Diagnostic {
        replacement: Option<ActorReferenceId>,
        remove: Option<ActorReferenceId>,
        fail: bool,
        calls: usize,
    }
    impl ActorWorldDiagnosticBackend for Diagnostic {
        fn message_beep(
            &mut self,
            w: &mut ActorWorld,
            owner: &mut Option<ActorReferenceId>,
            arg: i32,
        ) -> Result<(), LookupError> {
            assert_eq!(arg, -1);
            self.calls += 1;
            *owner = self.replacement;
            if let Some(a) = self.remove {
                w.unload_actor(Some(a));
            }
            if self.fail {
                return Err(LookupError::Backend("beep unavailable".into()));
            }
            Ok(())
        }
    }
    fn lookup<'a>(
        w: &'a mut ActorWorld,
        frames: &'a Frames,
        d: &'a mut Diagnostic,
    ) -> ActorWorldLookup<'a> {
        ActorWorldLookup {
            world: w,
            owner: None,
            frames,
            scene: None,
            diagnostic: d,
            diagnostic_default_c_locale: true,
        }
    }
    #[test]
    fn source_membership_preserves_identity_and_actual_death_rejects_reused_raw_identity() {
        let mut w = world();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 7);
        w.add_actor(Some(a), true, &mut e).unwrap();
        w.players[1] = Some(a);
        w.unload_actor(Some(a));
        let frames = Frames(vec![]);
        let mut d = Diagnostic::default();
        assert_eq!(lookup(&mut w, &frames, &mut d).actor_frame(a), Ok(None));
        w.destroy_base(a, &mut e).unwrap();
        let replacement = actor(&mut w, &mut e, 7);
        assert_ne!(a, replacement);
        let l = lookup(&mut w, &frames, &mut d);
        assert_eq!(l.actor_frame(a), Err(LookupError::StaleActor(a)));
        assert_eq!(l.actor_frame(replacement), Ok(None));
        assert_eq!(l.players().unwrap()[1], Some(a));
    }
    #[test]
    fn owned_lookup_observes_diagnostic_owner_and_collection_mutations() {
        let mut w = world();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        let b = actor(&mut w, &mut e, 2);
        let frames = Frames(vec![(
            FrameLifetime::for_existing(1),
            b"chcipnuti chlapka".to_vec(),
        )]);
        w.state_mut(a).unwrap().frame = Some(frames.0[0].0.reference());
        w.add_actor(Some(a), true, &mut e).unwrap();
        let mut d = Diagnostic {
            replacement: Some(b),
            remove: Some(a),
            ..Diagnostic::default()
        };
        let mut c = control();
        c.set_actor_reference(0, Some(a)).unwrap();
        {
            let mut l = lookup(&mut w, &frames, &mut d);
            l.owner = Some(a);
            script_lookup::find_actor(&mut c, &mut l, 0, b"").unwrap();
        }
        assert_eq!(c.actor_references()[0], Some(b));
        assert_eq!(d.calls, 1);
        assert!(w.source(ActorSource::Primary).is_empty());
    }
    #[test]
    fn diagnostic_failure_keeps_callback_mutations_but_preserves_script_slot() {
        let mut w = world();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        let b = actor(&mut w, &mut e, 2);
        let frames = Frames(vec![(
            FrameLifetime::for_existing(1),
            b"CHCIPNUTI CHLAPKA".to_vec(),
        )]);
        w.state_mut(a).unwrap().frame = Some(frames.0[0].0.reference());
        w.add_actor(Some(a), true, &mut e).unwrap();
        let mut d = Diagnostic {
            replacement: Some(b),
            remove: Some(a),
            fail: true,
            calls: 0,
        };
        let mut c = control();
        c.set_actor_reference(0, Some(a)).unwrap();
        {
            let mut l = lookup(&mut w, &frames, &mut d);
            l.owner = Some(a);
            assert!(script_lookup::find_actor(&mut c, &mut l, 0, b"").is_err());
            assert_eq!(l.owner, Some(b));
        }
        assert_eq!(c.actor_references()[0], Some(a));
        assert!(w.source(ActorSource::Primary).is_empty());
    }
    #[test]
    fn incomplete_actor_cannot_be_presented_as_live_lookup_result() {
        let mut w = world();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        let f = FrameLifetime::for_existing(1);
        w.state_mut(a).unwrap().frame = Some(f.reference());
        e.fail = Some("release");
        assert!(w.destroy_base(a, &mut e).is_err());
        let mut d = Diagnostic::default();
        let frames = Frames(vec![]);
        assert!(matches!(
            lookup(&mut w, &frames, &mut d).actor_frame(a),
            Err(LookupError::Backend(_))
        ));
    }
    #[test]
    fn factory_kind_one_uses_real_base_constructor_and_rejects_unsupported_kinds() {
        let mut w = world();
        let mut e = Effects::default();
        assert_eq!(
            w.create_actor(4, &mut e).unwrap_err().source,
            ActorWorldError::UnsupportedKind(4)
        );
        assert!(e.events.is_empty());
        let a = w.create_actor(1, &mut e).unwrap().unwrap();
        assert_eq!(a.identity(), 42);
        assert_eq!(e.events, [Event::AllocateActor(0x70)]);
        assert_eq!(w.state(a).unwrap().frame, None);
        assert!(w.source(ActorSource::Primary).is_empty());
    }
    #[test]
    fn factory_native_null_results_are_distinct_from_unsupported_kind_and_host_error() {
        let mut w = world();
        let mut e = Effects::default();
        for kind in [0, 39, u32::MAX] {
            assert_eq!(w.create_actor(kind, &mut e), Ok(None));
        }
        assert!(e.events.is_empty());
        e.fail = Some("null_actor");
        assert_eq!(w.create_actor(1, &mut e), Ok(None));
        assert_eq!(w.actor_count, 0);
        assert!(w.source(ActorSource::Static).is_empty());
        e.fail = Some("allocate_actor");
        assert!(w.create_actor(1, &mut e).is_err());
        assert_eq!(w.actor_count, 0);
    }
    fn detector_seed() -> DetectorSeed {
        DetectorSeed {
            program: ProgramSeed {
                address_high_byte: 0xff,
                padding_0d: [1; 3],
                words_1c_2c: [2; 5],
                padding_31: [3; 3],
                word_40: 4,
                bytes_64: [5; 4],
            },
            bytes_d8_ef: [0xa5; 24],
            word_f4: 0x12345678,
            bytes_105_107: [0x63; 3],
            bytes_118_127: [0x72; 16],
        }
    }
    #[derive(Default)]
    struct ProgramEffects {
        next: u64,
        frees: Vec<(crate::original_program::ProgramBuffer, Option<u64>)>,
        actor_frees: Vec<u32>,
        fail_free: bool,
    }
    impl ProgramBackend for ProgramEffects {
        fn allocate(
            &mut self,
            _: &CProgram,
            _: crate::original_program::ProgramBuffer,
            _: u32,
        ) -> Result<crate::original_program::ProgramAllocation, String> {
            self.next += 1;
            Ok(crate::original_program::ProgramAllocation(self.next))
        }
        fn free(
            &mut self,
            _: &CProgram,
            b: crate::original_program::ProgramBuffer,
            a: Option<crate::original_program::ProgramAllocation>,
        ) -> Result<(), String> {
            self.frees.push((b, a.map(|a| a.0)));
            if self.fail_free {
                Err("unavailable free".into())
            } else {
                Ok(())
            }
        }
    }
    impl ActorDeallocationBackend for ProgramEffects {
        fn free_actor(&mut self, world: &ActorWorld, identity: u32) -> Result<(), String> {
            assert_eq!(world.actor_count, 0);
            self.actor_frees.push(identity);
            Ok(())
        }
    }
    fn detector_payload(source: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0xff, 0x12, 0x34, 0x56, 0x78, 0xfe];
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&(source.len() as u32).to_le_bytes());
        bytes.extend_from_slice(source);
        bytes
    }
    #[test]
    fn detector_owns_real_frame_init_payload_replacement_and_release() {
        use crate::actor_frames::{FrameUserData, ModelActorFrames};
        use crate::frame_lookup::ModelFrameLookup;
        use crate::model_loading::{tests::frame, MaterializedModel};
        use crate::original_program::{ProgramPhase, ProgramSliceReader};
        let mut source = frame(6, None, [0.; 3]);
        source.name_bytes = b"actual.full.frame".to_vec();
        let mut model = MaterializedModel::from_parsed(std::sync::Arc::new(lhr_assets::Model {
            materials: vec![],
            frames: vec![source],
        }))
        .unwrap();
        let key = model.source_frame_keys()[0];
        let frame_ref = ModelFrameLookup::borrow(model.arena(), true)
            .reference(key)
            .unwrap();
        let record = model.arena_mut().create_actor_data_record(None).unwrap();
        model
            .arena_mut()
            .set_user_data(key, FrameUserData::ActorRecord(record))
            .unwrap();
        let mut w = world();
        let mut e = Effects::default();
        let a = w
            .construct_detector(0x12ffffc0, seed(), detector_seed(), &mut e)
            .unwrap();
        let d = w.detector(a).unwrap();
        assert_eq!(d.program().unwrap().seed().address_high_byte, 0x13);
        assert_eq!(d.bytes_d8_ef, [0xa5; 24]);
        assert_eq!(
            (d.word_f0, d.words_f8_100, d.byte_104, d.words_108_114),
            (0, [0; 3], 0, [0; 4])
        );
        assert_eq!(d.program().unwrap().owner(), None);
        assert!(w
            .init(
                a,
                Some(frame_ref),
                &mut ModelActorFrames::borrow(model.arena_mut(), true)
            )
            .unwrap());
        assert_eq!(w.detector(a).unwrap().program().unwrap().owner(), Some(a));
        assert_eq!(w.detector(a).unwrap().data_d8(), 0);
        assert_eq!(model.arena().user_data(key), Ok(FrameUserData::Null));
        let mut p = ProgramEffects::default();
        let replacement = ProgramReplacement {
            mission: Some(b"mission".to_vec()),
            name: None,
            frame: Some(b"ACTUAL.FULL.FRAME".to_vec()),
            source: Some(b"replacement\0ignored".to_vec()),
        };
        w.load_detector_data(
            a,
            &mut ProgramSliceReader::new(&detector_payload(b"uncompiled source")),
            &[replacement],
            Some(b"real\\mission"),
            &ModelFrameLookup::borrow(model.arena(), true),
            &mut p,
        )
        .unwrap();
        let d = w.detector(a).unwrap();
        assert_eq!(d.data_d8(), 0x78563412);
        assert_eq!(
            d.program().unwrap().source().unwrap().c_bytes(),
            Some(&b"replacement"[..])
        );
        assert_eq!(d.program().unwrap().phase(), ProgramPhase::Source);
        assert_eq!(model.arena().actor_data_record(record), Ok(None));
        assert_eq!(
            w.release_detector(
                a,
                &mut ModelActorFrames::borrow(model.arena_mut(), true),
                &mut p
            ),
            Ok(0)
        );
        assert_eq!(w.inspect(a).unwrap().0, ActorPhase::Dead);
        assert_eq!(
            w.inspect_detector(a).unwrap().program().unwrap().phase(),
            ProgramPhase::Dead
        );
        assert_eq!(p.actor_frees, [a.identity()]);
        assert!(w.source(ActorSource::Static).is_empty());
        assert!(model.arena().frame(key).is_ok());
    }
    #[test]
    fn detector_base_bypass_and_failed_source_free_cannot_retire_actor() {
        use crate::original_program::ProgramSliceReader;
        let mut w = world();
        let mut e = Effects::default();
        let a = w
            .construct_detector(9, seed(), detector_seed(), &mut e)
            .unwrap();
        assert_eq!(w.state_mut(a), Err(ActorWorldError::UnsupportedVtable));
        // Even a corrupted tag cannot change the actual storage owner.
        let i = w.index(a).unwrap();
        w.actors[i].state.vtable = ActorVtable::Base;
        assert_eq!(
            w.destroy_base(a, &mut e),
            Err(ActorWorldError::UnsupportedVtable)
        );
        assert_eq!(
            w.release(a, &mut e),
            Err(ActorWorldError::UnsupportedVtable)
        );
        assert_eq!(w.state(a).unwrap().references, 1);
        let mut p = ProgramEffects::default();
        w.load_detector_data(
            a,
            &mut ProgramSliceReader::new(&detector_payload(b"source")),
            &[],
            None,
            &Frames(vec![]),
            &mut p,
        )
        .unwrap();
        p.fail_free = true;
        assert!(w.destroy_detector(a, &mut e, &mut p).is_err());
        assert_eq!(w.phase(a), Ok(ActorPhase::DestructionInterrupted));
        assert_eq!(w.actor_count, 1);
        assert_eq!(w.source(ActorSource::Static), &[Some(a)]);
        assert!(matches!(
            w.destroy_detector(a, &mut e, &mut p),
            Err(ActorWorldError::Busy(_))
        ));
        assert_eq!(p.frees.len(), 1);
    }
    #[test]
    fn interrupted_detector_load_keeps_rawfield_and_can_dispose_owned_prefix() {
        use crate::original_program::ProgramSliceReader;
        let mut w = world();
        let mut e = Effects::default();
        let a = w
            .construct_detector(9, seed(), detector_seed(), &mut e)
            .unwrap();
        let mut p = ProgramEffects::default();
        let bytes = detector_payload(b"code");
        assert!(w
            .load_detector_data(
                a,
                &mut ProgramSliceReader::new(&bytes[..bytes.len() - 1]),
                &[],
                None,
                &Frames(vec![]),
                &mut p
            )
            .is_err());
        assert_eq!(w.phase(a), Ok(ActorPhase::LoadingInterrupted));
        assert_eq!(w.detector(a).unwrap().data_d8(), 0x78563412);
        assert!(
            !w.detector(a)
                .unwrap()
                .program()
                .unwrap()
                .source()
                .unwrap()
                .complete
        );
        w.destroy_detector(a, &mut e, &mut p).unwrap();
        assert_eq!(w.actor_count, 0);
    }
    #[test]
    fn detector_factory_registers_base_before_derived_construction() {
        struct Factory;
        impl ActorAllocationBackend for Factory {
            fn allocate_actor(&mut self, _: u32) -> Result<Option<(u32, BaseActorSeed)>, String> {
                panic!("kind5")
            }
            fn allocate_detector(
                &mut self,
                bytes: u32,
            ) -> Result<Option<(u32, BaseActorSeed, DetectorSeed)>, String> {
                assert_eq!(bytes, 0x128);
                Ok(Some((0x10203040, seed(), detector_seed())))
            }
        }
        impl ActorCollectionBackend for Factory {
            fn allocate_collection(
                &mut self,
                world: &ActorWorld,
                source: ActorSource,
                bytes: u32,
            ) -> Result<ActorStorageId, String> {
                assert_eq!(
                    (source, bytes, world.actor_count),
                    (ActorSource::Static, 4, 1)
                );
                let actor = &world.actors[0];
                assert_eq!(actor.phase, ActorPhase::Constructing);
                assert_eq!(actor.state.vtable, ActorVtable::Base);
                assert_eq!(actor.state.word_10, 1);
                assert!(actor.detector.is_none());
                Ok(ActorStorageId(33))
            }
            fn free_collection(
                &mut self,
                _: &ActorWorld,
                _: ActorSource,
                old: Option<ActorStorageId>,
            ) -> Result<(), String> {
                assert_eq!(old, None);
                Ok(())
            }
        }
        let mut w = ActorWorld::new(0, 0, backing(0), backing(0), backing(0)).unwrap();
        let a = w.create_actor(5, &mut Factory).unwrap().unwrap();
        assert_eq!(w.state(a).unwrap().vtable, ActorVtable::Detector);
        assert_eq!(w.state(a).unwrap().word_10, 5);
        assert_eq!(w.detector(a).unwrap().program().unwrap().owner(), None);
    }
    #[test]
    fn detector_init_keeps_derived_identity_while_base_callbacks_change_attachment() {
        let mut w = world();
        let mut e = Effects::default();
        let a = w
            .construct_detector(9, seed(), detector_seed(), &mut e)
            .unwrap();
        let first = FrameLifetime::for_existing(21);
        let second = FrameLifetime::for_existing(22);
        e.actor = Some(a);
        e.replace_after_e = Some(Some(second.reference()));
        assert_eq!(w.init(a, Some(first.reference()), &mut e), Ok(true));
        assert_eq!(w.state(a).unwrap().vtable, ActorVtable::Detector);
        assert_eq!(w.state(a).unwrap().word_10, 5);
        assert_eq!(w.state(a).unwrap().frame, Some(second.reference()));
        assert!(e.events.iter().any(|event| matches!(event, Event::Find(frame, name, ..) if *frame == second.reference() && name == b"targetN")));
        assert_eq!(w.detector(a).unwrap().program().unwrap().owner(), Some(a));
        let b = w
            .construct_detector(10, seed(), detector_seed(), &mut e)
            .unwrap();
        e.actor = Some(b);
        e.replace_after_e = Some(None);
        assert_eq!(
            w.init(b, Some(first.reference()), &mut e),
            Err(ActorWorldError::MissingAttachedFrame)
        );
        assert_eq!(w.phase(b), Ok(ActorPhase::InitializationInterrupted));
        assert_eq!(w.detector(b).unwrap().program().unwrap().owner(), None);
        assert_eq!(w.detector(b).unwrap().data_d8(), 0xa5a5a5a5);
    }
    #[test]
    fn detector_load_uncertain_disposal_is_never_replayed_by_teardown() {
        use crate::original_program::{ProgramPhase, ProgramSliceReader};
        let mut w = world();
        let mut e = Effects::default();
        let a = w
            .construct_detector(9, seed(), detector_seed(), &mut e)
            .unwrap();
        let mut p = ProgramEffects::default();
        w.load_detector_data(
            a,
            &mut ProgramSliceReader::new(&detector_payload(b"old")),
            &[],
            None,
            &Frames(vec![]),
            &mut p,
        )
        .unwrap();
        p.fail_free = true;
        assert!(w
            .load_detector_data(
                a,
                &mut ProgramSliceReader::new(&detector_payload(b"new")),
                &[],
                None,
                &Frames(vec![]),
                &mut p
            )
            .is_err());
        assert_eq!(
            w.detector(a).unwrap().program().unwrap().phase(),
            ProgramPhase::Poisoned
        );
        assert_eq!(w.phase(a), Ok(ActorPhase::LoadingInterrupted));
        assert_eq!(p.frees.len(), 1);
        assert_eq!(
            w.destroy_detector(a, &mut e, &mut p),
            Err(ActorWorldError::Program(ProgramError::Unavailable))
        );
        assert_eq!(w.phase(a), Ok(ActorPhase::DestructionInterrupted));
        assert_eq!(p.frees.len(), 1);
        assert_eq!(w.actor_count, 1);
        assert_eq!(w.source(ActorSource::Static), &[Some(a)]);
        assert!(p.actor_frees.is_empty());
    }
}
