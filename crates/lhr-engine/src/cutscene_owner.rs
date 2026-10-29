//! Bounded live PC cutscene owner and LoadFull resource-dictionary prefix.
//! Game SHA256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27:
//! constructor RVA19cc40, resource constructor19b5f0, manager constructor19cf70,
//! dictionary reader19bab0, LoadFull19a3c0. No playback or native Close is supplied.
#![forbid(unsafe_code)]

use crate::actor_world::{
    ActorAssociation, ActorSource, ActorWorld, ActorWorldError, ActorWorldIdentity,
};
use crate::cutscene_records::{
    CutsceneRecordError, CutsceneRecordInputError, CutsceneRecordInputs, RecordManagerId,
    RecordedActorAssociation, RecordedActorId,
};
use crate::cutscene_source::{CutsceneSource, CutsceneSourceHeader, CutsceneSourceReadError};
use crate::frame_lookup::{FrameLookupBackend, FrameLookupError};
use crate::script_scheduler::ActorReferenceId;

/// Safe host allocation domain, not a native recording limit.
const MAX_RESOURCE_ENTRIES: u32 = 65_536;

/// Untouched allocator temporaries, retained as narrow constructor provenance.
/// They are not initialized zero by the original constructor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CutsceneConstructorSeed {
    pub outer_allocator_byte: u8,
    pub manager_address_high_byte: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CutscenePrefixPhase {
    ConstructedStopped,
    ReadingHeader,
    ReadingResources,
    BeforeActorDescriptorAllocation,
    ReadingRecordInputs,
    BeforeRecordedActorIteration,
    BindingRecordedActor,
    AfterFirstRecordedActorBinding,
    Interrupted,
}

/// Reached source-read phase; original call-site provenance stays in comments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CutscenePrefixReadSite {
    Header,         // Game59a404
    ResourceCount,  // Game59bac1
    ResourceGrowth, // Game59bacd
    ResourceId,     // Game59bb16
    ResourceName,   // Game59bb26
}

#[derive(Debug, PartialEq, Eq)]
pub enum CutscenePrefixError {
    NotFresh,
    Read {
        site: CutscenePrefixReadSite,
        error: CutsceneSourceReadError,
    },
    BadMagic(u32),
    ResourceCountOutsideDomain(i32),
    HostAllocation,
    RecordInput(CutsceneRecordInputError),
    RecordOwner(CutsceneRecordError),
    ManagerNotAdmitted,
    ForeignManager,
    ManagerNotFresh,
    Actor(ActorWorldError),
    Frame(FrameLookupError),
    MissingActorFrame,
    UnterminatedFrameName,
    FrameNameOutsideDomain,
    RequiredRecordedActor {
        frame_name: Vec<u8>,
    },
    RequiredNullActorBinding,
    RequiredExistingRecordingBinding,
    RequiredEmptyDescriptorContinuation,
}

/// Required recorded-actor pipeline, never LoadFull success or playing.
/// This prefix stops BEFORE descriptor allocation, not at an actor binding call.
#[derive(Debug, PartialEq, Eq)]
pub struct BeforeActorDescriptorAllocation {
    /// The next native phase allocates the serialized actor-descriptor table.
    pub descriptor_count: u32,
}

/// Resource ingestion completed; actor iteration and record binding remain
/// unavailable. This is earlier than59d740 and is not a successful LoadFull.
#[derive(Debug, PartialEq, Eq)]
pub struct BeforeRecordedActorIteration {
    pub descriptor_count: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub struct AfterFirstRecordedActorBinding {
    pub actor: ActorReferenceId,
    pub record: RecordedActorId,
}

/// One allocated dictionary slot. Absent/incomplete fields have no claimed
/// native value. Names are fixed file records, not fabricated resource objects.
#[derive(Debug)]
pub struct CutsceneResourceEntry {
    id_bytes: [u8; 4],
    id_bytes_read: usize,
    name_bytes: [u8; 48],
    name_bytes_read: usize,
    binding_initialized_null: bool,
}
impl CutsceneResourceEntry {
    fn unread() -> Self {
        Self {
            id_bytes: [0; 4],
            id_bytes_read: 0,
            name_bytes: [0; 48],
            name_bytes_read: 0,
            binding_initialized_null: false,
        }
    }
    pub fn id(&self) -> Option<u32> {
        (self.id_bytes_read == 4).then(|| u32::from_le_bytes(self.id_bytes))
    }
    pub fn id_prefix(&self) -> &[u8] {
        &self.id_bytes[..self.id_bytes_read]
    }
    pub fn name_prefix(&self) -> &[u8] {
        &self.name_bytes[..self.name_bytes_read]
    }
    pub fn name_record(&self) -> Option<&[u8; 48]> {
        (self.name_bytes_read == 48).then_some(&self.name_bytes)
    }
    /// Native dictionary reader stores the unresolved link word to null only
    /// after this entry's id and name reads have completed.
    pub fn binding_is_initialized_null(&self) -> bool {
        self.binding_initialized_null
    }
}

// Constructor-only element domains. No populated value is admitted until its
// actual producer/ownership contract exists; these vectors genuinely start empty.
#[derive(Debug)]
enum RetainedCutsceneObject {}
#[derive(Debug)]
enum RecordedAuxiliaryEntry {}

/// Constructor-established part of the original singleton. Fields outside
/// this projection stay unavailable; no full-object layout/function credit.
#[derive(Debug)]
pub struct CutsceneOwner {
    seed: CutsceneConstructorSeed,
    auxiliary_words: Vec<u32>,
    retained_objects: Vec<RetainedCutsceneObject>,
    manager_auxiliary: Vec<RecordedAuxiliaryEntry>,
    phase: CutscenePrefixPhase,
    state: i32,
    previous_state: i32,
    time: u32,
    playback_time: u32,
    speed: f32,
    resource_count: Option<i32>,
    resource_growth: Option<i32>,
    pending_dictionary_word: Vec<u8>,
    resources: Vec<CutsceneResourceEntry>,
    header_bytes: [u8; 96],
    header_bytes_read: usize,
    header: Option<CutsceneSourceHeader>,
    source: Option<CutsceneSource>,
    closed_source_position: Option<usize>,
    record_inputs: Option<CutsceneRecordInputs>,
    record_manager: Option<RecordManagerId>,
    actor_context: Option<ActorWorldIdentity>,
    pending_record: Option<RecordedActorId>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CutscenePrefixDiscard {
    pub source_bytes_released: usize,
    pub dictionary_slots_released: usize,
    /// This owner cannot release resources transferred to ActorWorld.manager.
    pub manager_cleanup_required: bool,
}

impl CutsceneOwner {
    pub fn constructed(seed: CutsceneConstructorSeed) -> Self {
        Self {
            seed,
            auxiliary_words: Vec::new(),
            retained_objects: Vec::new(),
            manager_auxiliary: Vec::new(),
            phase: CutscenePrefixPhase::ConstructedStopped,
            state: 4,
            previous_state: 0,
            time: 0,
            playback_time: 0,
            speed: 1.,
            resource_count: Some(0),
            resource_growth: Some(0x100),
            pending_dictionary_word: Vec::new(),
            resources: Vec::new(),
            header_bytes: [0; 96],
            header_bytes_read: 0,
            header: None,
            source: None,
            closed_source_position: None,
            record_inputs: None,
            record_manager: None,
            actor_context: None,
            pending_record: None,
        }
    }
    pub fn seed(&self) -> CutsceneConstructorSeed {
        self.seed
    }
    pub fn phase(&self) -> CutscenePrefixPhase {
        self.phase
    }
    pub fn state(&self) -> i32 {
        self.state
    }
    pub fn previous_state(&self) -> i32 {
        self.previous_state
    }
    pub fn clocks(&self) -> (u32, u32, f32) {
        (self.time, self.playback_time, self.speed)
    }
    pub fn actor_counts(&self, world: &ActorWorld) -> (usize, usize) {
        (world.manager.first.len(), world.manager.second.len())
    }
    pub fn constructor_collection_counts(&self, world: &ActorWorld) -> [usize; 6] {
        [
            world.manager.first.len(),
            world.manager.second.len(),
            self.auxiliary_words.len(),
            self.retained_objects.len(),
            world.manager.associations.len(),
            self.manager_auxiliary.len(),
        ]
    }
    pub fn resource_count(&self) -> Option<i32> {
        self.resource_count
    }
    pub fn resource_growth(&self) -> Option<i32> {
        self.resource_growth
    }
    pub fn pending_dictionary_word(&self) -> &[u8] {
        &self.pending_dictionary_word
    }
    pub fn resources(&self) -> &[CutsceneResourceEntry] {
        &self.resources
    }
    pub fn header(&self) -> Option<&CutsceneSourceHeader> {
        self.header.as_ref()
    }
    pub fn header_prefix(&self) -> &[u8] {
        &self.header_bytes[..self.header_bytes_read]
    }
    pub fn source_position(&self) -> Option<usize> {
        self.source
            .as_ref()
            .map(CutsceneSource::position)
            .or(self.closed_source_position)
    }

    /// Connects the fresh native manager projection to the SAME ActorWorld
    /// before any header/dictionary effects. Existing populated manager cleanup
    /// is outside this bounded fresh-load domain.
    pub fn load_full_prefix_in_world(
        &mut self,
        source: CutsceneSource,
        world: &ActorWorld,
    ) -> Result<BeforeActorDescriptorAllocation, CutscenePrefixError> {
        if !world.manager.records.is_fresh()
            || !world.manager.associations.is_empty()
            || !world.manager.first.is_empty()
            || !world.manager.second.is_empty()
        {
            return Err(CutscenePrefixError::ManagerNotFresh);
        }
        if self.phase != CutscenePrefixPhase::ConstructedStopped {
            return Err(CutscenePrefixError::NotFresh);
        }
        self.record_manager = Some(world.manager.records.identity());
        self.actor_context = Some(world.identity());
        self.load_full_prefix(source)
    }

    /// Begins only on the constructor's fresh/stopped domain. The supplied
    /// source is already acquired; bad acquisition therefore cannot mutate us.
    /// Even a complete prefix returns a required dependency, never true/playing.
    pub fn load_full_prefix(
        &mut self,
        source: CutsceneSource,
    ) -> Result<BeforeActorDescriptorAllocation, CutscenePrefixError> {
        if self.phase != CutscenePrefixPhase::ConstructedStopped {
            return Err(CutscenePrefixError::NotFresh);
        }
        self.source = Some(source);
        self.phase = CutscenePrefixPhase::ReadingHeader;
        let source = self.source.as_mut().unwrap();
        let before = source.position();
        let result = source.read_exact(&mut self.header_bytes);
        self.header_bytes_read = source.position() - before;
        if let Err(error) = result {
            self.phase = CutscenePrefixPhase::Interrupted;
            return Err(CutscenePrefixError::Read {
                site: CutscenePrefixReadSite::Header,
                error,
            });
        }
        let header = CutsceneSourceHeader::from_bytes(self.header_bytes);
        if header.magic() != 0x1631 {
            let magic = header.magic();
            // Native closes the input before returning false on a bad magic.
            // Dropping this buffered input makes no native dtaClose claim.
            self.source = None;
            self.phase = CutscenePrefixPhase::Interrupted;
            return Err(CutscenePrefixError::BadMagic(magic));
        }
        self.header = Some(header);
        // Native59a430 clears manager entries, then59a43a removes owner actors.
        // Constructor domain proves all those lists/backing links empty; no
        // actor Reset/GameDone callback is silently substituted here.
        self.phase = CutscenePrefixPhase::ReadingResources;
        if let Err(error) = self.read_resources() {
            self.phase = CutscenePrefixPhase::Interrupted;
            return Err(error);
        }
        self.phase = CutscenePrefixPhase::BeforeActorDescriptorAllocation;
        Ok(BeforeActorDescriptorAllocation {
            descriptor_count: self.header.as_ref().unwrap().actor_descriptor_count(),
        })
    }

    /// Continues the actual LoadFull allocation/read order after the dictionary.
    /// Reaching the recorded-actor iteration is a required dependency, never playback success.
    pub fn read_record_inputs(
        &mut self,
        world: &mut ActorWorld,
    ) -> Result<BeforeRecordedActorIteration, CutscenePrefixError> {
        if self.phase != CutscenePrefixPhase::BeforeActorDescriptorAllocation {
            return Err(CutscenePrefixError::NotFresh);
        }
        let admitted = self
            .record_manager
            .as_ref()
            .ok_or(CutscenePrefixError::ManagerNotAdmitted)?;
        if *admitted != world.manager.records.identity()
            || self.actor_context != Some(world.identity())
        {
            return Err(CutscenePrefixError::ForeignManager);
        }
        if !world.manager.records.is_fresh()
            || !world.manager.associations.is_empty()
            || !world.manager.first.is_empty()
            || !world.manager.second.is_empty()
        {
            return Err(CutscenePrefixError::ManagerNotFresh);
        }
        self.phase = CutscenePrefixPhase::ReadingRecordInputs;
        self.record_inputs = Some(CutsceneRecordInputs::default());
        let result = self
            .record_inputs
            .as_mut()
            .unwrap()
            .read_allocated_inputs(self.source.as_mut().unwrap(), self.header.as_ref().unwrap());
        if let Err(error) = result {
            self.phase = CutscenePrefixPhase::Interrupted;
            return Err(CutscenePrefixError::RecordInput(error));
        }
        // Native59a564 closes the source before59a57a publishes the pool into
        // the fresh manager. This buffered adapter releases its input here.
        self.closed_source_position = self.source.as_ref().map(CutsceneSource::position);
        self.source = None;
        if let Err(error) = self
            .record_inputs
            .as_mut()
            .unwrap()
            .publish_manager_pool(&mut world.manager.records)
        {
            self.phase = CutscenePrefixPhase::Interrupted;
            return Err(CutscenePrefixError::RecordOwner(error));
        }
        self.phase = CutscenePrefixPhase::BeforeRecordedActorIteration;
        Ok(BeforeRecordedActorIteration {
            descriptor_count: self.header.as_ref().unwrap().actor_descriptor_count(),
        })
    }
    pub fn record_inputs(&self) -> Option<&CutsceneRecordInputs> {
        self.record_inputs.as_ref()
    }

    /// Executes the first descriptor's first actual matching actor binding.
    /// Missing original actors, null actors and replacement-name paths remain
    /// explicit unavailable continuations. A completed bind is still state4 and
    /// stops before subsequent actor matches/descriptors or camera publication.
    pub fn bind_first_recorded_actor(
        &mut self,
        world: &mut ActorWorld,
        frames: &dyn FrameLookupBackend,
    ) -> Result<AfterFirstRecordedActorBinding, CutscenePrefixError> {
        if self.phase != CutscenePrefixPhase::BeforeRecordedActorIteration {
            return Err(CutscenePrefixError::NotFresh);
        }
        if self.actor_context != Some(world.identity())
            || self.record_manager.as_ref() != Some(&world.manager.records.identity())
        {
            return Err(CutscenePrefixError::ForeignManager);
        }
        self.phase = CutscenePrefixPhase::BindingRecordedActor;
        let result = self.bind_first_inner(world, frames);
        self.phase = if result.is_ok() {
            CutscenePrefixPhase::AfterFirstRecordedActorBinding
        } else {
            CutscenePrefixPhase::Interrupted
        };
        result
    }
    fn bind_first_inner(
        &mut self,
        world: &mut ActorWorld,
        frames: &dyn FrameLookupBackend,
    ) -> Result<AfterFirstRecordedActorBinding, CutscenePrefixError> {
        let inputs = self.record_inputs.as_ref().unwrap();
        let descriptor = inputs
            .descriptor(0)
            .ok_or(CutscenePrefixError::RequiredEmptyDescriptorContinuation)?;
        let pool = inputs.published_pool().unwrap();
        let frame_record = descriptor.frame_name_record();
        let frame_name_len = frame_record
            .iter()
            .position(|b| *b == 0)
            .ok_or(CutscenePrefixError::UnterminatedFrameName)?;
        let frame_name = &frame_record[..frame_name_len];
        if frame_name == b"<none>" {
            let record = world
                .manager
                .records
                .allocate_record(&descriptor, pool, 0)
                .map_err(CutscenePrefixError::RecordOwner)?;
            self.pending_record = Some(record);
            return Err(CutscenePrefixError::RequiredNullActorBinding);
        }
        let mut index = 0;
        let actor = loop {
            // Native captures this context at LoadFull entry; the live Primary
            // array and actor/frame identities are read at each reached match.
            let Some(candidate) = world.source(ActorSource::Primary).get(index) else {
                return Err(CutscenePrefixError::RequiredRecordedActor {
                    frame_name: frame_name.to_vec(),
                });
            };
            let candidate = candidate.ok_or(CutscenePrefixError::MissingActorFrame)?;
            let frame = world
                .state(candidate)
                .map_err(CutscenePrefixError::Actor)?
                .frame
                .ok_or(CutscenePrefixError::MissingActorFrame)?;
            let view = frames.frame(frame).map_err(CutscenePrefixError::Frame)?;
            if view.name.unwrap_or(b"") == frame_name {
                break candidate;
            }
            index += 1;
        };
        let record = world
            .manager
            .records
            .allocate_record(&descriptor, pool, 0)
            .map_err(CutscenePrefixError::RecordOwner)?;
        self.pending_record = Some(record.clone());
        let recording_name = world
            .manager
            .records
            .record(&record)
            .map_err(CutscenePrefixError::RecordOwner)?
            .recording_name()
            .unwrap();
        for association in &world.manager.associations {
            match association {
                ActorAssociation::Recorded(entry) if entry.recording_name == recording_name => {
                    return Err(CutscenePrefixError::RequiredExistingRecordingBinding);
                }
                ActorAssociation::OpaqueLegacy { .. } => {
                    return Err(CutscenePrefixError::RequiredExistingRecordingBinding);
                }
                _ => {}
            }
        }
        world
            .attach_recorded_actor(actor)
            .map_err(CutscenePrefixError::Actor)?;
        //59d7f8 rereads actor.frame and its actual name AFTER mode callbacks.
        let frame = world
            .state(actor)
            .map_err(CutscenePrefixError::Actor)?
            .frame
            .ok_or(CutscenePrefixError::MissingActorFrame)?;
        let current_frame_name = frames
            .frame(frame)
            .map_err(CutscenePrefixError::Frame)?
            .name
            .unwrap_or(b"");
        if current_frame_name.len() >= 36 {
            return Err(CutscenePrefixError::FrameNameOutsideDomain);
        }
        let entry = RecordedActorAssociation {
            uses_record_pool: true,
            recorded_actor_type: descriptor.recorded_actor_type(),
            frame_name: current_frame_name.to_vec(),
            recording_name: world
                .manager
                .records
                .record(&record)
                .map_err(CutscenePrefixError::RecordOwner)?
                .recording_name()
                .unwrap()
                .to_vec(),
            actor: Some(actor),
            record: Some(record.clone()),
        };
        world
            .set_recorded_actor_data(actor, record.clone())
            .map_err(CutscenePrefixError::Actor)?;
        world
            .append_recorded_association(entry)
            .map_err(CutscenePrefixError::Actor)?;
        world
            .manager
            .records
            .initialize_actor_playback(&record)
            .map_err(CutscenePrefixError::RecordOwner)?;
        Ok(AfterFirstRecordedActorBinding { actor, record })
    }
    pub fn pending_record(&self) -> Option<&RecordedActorId> {
        self.pending_record.as_ref()
    }

    fn read_resources(&mut self) -> Result<(), CutscenePrefixError> {
        let source = self.source.as_mut().unwrap();
        let mut word = [0; 4];
        let result = source.read_exact(&mut word);
        if let Err(error) = result {
            if error.copied != 0 {
                self.resource_count = None;
                self.pending_dictionary_word
                    .extend_from_slice(&word[..error.copied]);
            }
            return Err(CutscenePrefixError::Read {
                site: CutscenePrefixReadSite::ResourceCount,
                error,
            });
        }
        let count = i32::from_le_bytes(word);
        self.resource_count = Some(count);
        let result = source.read_exact(&mut word);
        if let Err(error) = result {
            if error.copied != 0 {
                self.resource_growth = None;
                self.pending_dictionary_word
                    .extend_from_slice(&word[..error.copied]);
            }
            return Err(CutscenePrefixError::Read {
                site: CutscenePrefixReadSite::ResourceGrowth,
                error,
            });
        }
        self.resource_growth = Some(i32::from_le_bytes(word));
        if count < 0 || count as u32 > MAX_RESOURCE_ENTRIES {
            return Err(CutscenePrefixError::ResourceCountOutsideDomain(count));
        }
        let count = count as usize;
        // Native publication of allocated backing precedes per-entry reads.
        self.resources
            .try_reserve_exact(count)
            .map_err(|_| CutscenePrefixError::HostAllocation)?;
        self.resources
            .resize_with(count, CutsceneResourceEntry::unread);
        for entry in &mut self.resources {
            let before = source.position();
            let result = source.read_exact(&mut entry.id_bytes);
            entry.id_bytes_read = source.position() - before;
            result.map_err(|error| CutscenePrefixError::Read {
                site: CutscenePrefixReadSite::ResourceId,
                error,
            })?;
            let before = source.position();
            let result = source.read_exact(&mut entry.name_bytes);
            entry.name_bytes_read = source.position() - before;
            result.map_err(|error| CutscenePrefixError::Read {
                site: CutscenePrefixReadSite::ResourceName,
                error,
            })?;
            entry.binding_initialized_null = true;
        }
        Ok(())
    }

    /// Explicit terminal host cleanup for this interrupted prefix, NOT native
    /// C_cutscene::Close (which requires actual game/HUD/actor dependencies).
    /// Releases input allocations retained here. A transferred manager pool and
    /// retained record objects require separate terminal ActorWorld cleanup;
    /// no native actor cleanup or playing-state transition is performed.
    pub fn discard_prefix(self) -> CutscenePrefixDiscard {
        CutscenePrefixDiscard {
            source_bytes_released: self.source.as_ref().map_or(0, CutsceneSource::source_len),
            dictionary_slots_released: self.resources.len(),
            manager_cleanup_required: self
                .record_inputs
                .as_ref()
                .is_some_and(|inputs| inputs.published_pool().is_some()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_world() -> ActorWorld {
        let backing = crate::actor_world::SourceBacking {
            storage: None,
            capacity: 0,
        };
        ActorWorld::new(0, 0, backing, backing, backing).unwrap()
    }
    fn owner() -> CutsceneOwner {
        CutsceneOwner::constructed(CutsceneConstructorSeed {
            outer_allocator_byte: 0xa7,
            manager_address_high_byte: 0x3c,
        })
    }
    fn header(actors: u32) -> Vec<u8> {
        let mut bytes = vec![0x6d; 96];
        bytes[..4].copy_from_slice(&0x1631u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&actors.to_le_bytes());
        bytes
    }
    fn recording() -> Vec<u8> {
        let mut bytes = header(9);
        bytes.extend(2i32.to_le_bytes());
        bytes.extend(713i32.to_le_bytes());
        for id in [42u32, 42] {
            bytes.extend(id.to_le_bytes());
            let mut name = [0xfe; 48];
            name[..7].copy_from_slice(b"input\0x");
            bytes.extend(name);
        }
        bytes.extend([1, 2, 3]); // Unreached descriptor bytes must not be parsed.
        bytes
    }

    struct NoGrowth;
    impl crate::actor_world::ActorCollectionBackend for NoGrowth {
        fn allocate_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: u32,
        ) -> Result<crate::actor_world::ActorStorageId, String> {
            panic!("preallocated fixture")
        }
        fn free_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: Option<crate::actor_world::ActorStorageId>,
        ) -> Result<(), String> {
            panic!("preallocated fixture")
        }
    }
    fn binding_fixture(
        primary: bool,
        pool_bytes: usize,
    ) -> (
        ActorWorld,
        crate::model_loading::MaterializedModel,
        ActorReferenceId,
        Vec<u8>,
    ) {
        use crate::actor_frames::ModelActorFrames;
        use crate::actor_world::{ActorStorageId, BaseActorSeed, SourceBacking};
        use crate::frame_lookup::ModelFrameLookup;
        use crate::model_loading::{tests::frame, MaterializedModel};
        let backing = SourceBacking {
            storage: Some(ActorStorageId(7)),
            capacity: 8,
        };
        let mut world = ActorWorld::new(0, 0, backing, backing, backing).unwrap();
        let actor = world
            .construct_base(
                11,
                BaseActorSeed {
                    word_0e: 0,
                    actor_id: 9,
                    state_1c: 7,
                    bytes_21_5f: [0; 63],
                    bytes_64_65: [0; 2],
                    byte_67: 0,
                    release_frame: 0,
                    bytes_6d_6f: [0; 3],
                },
                &mut NoGrowth,
            )
            .unwrap();
        let mut source = frame(6, None, [0.; 3]);
        source.name_bytes = b"owned.frame".to_vec();
        let mut model = MaterializedModel::from_parsed(std::sync::Arc::new(lhr_assets::Model {
            materials: vec![],
            frames: vec![source],
        }))
        .unwrap();
        let reference = ModelFrameLookup::borrow(model.arena(), true)
            .reference(model.source_frame_keys()[0])
            .unwrap();
        assert!(world
            .init(
                actor,
                Some(reference),
                &mut ModelActorFrames::borrow(model.arena_mut(), true)
            )
            .unwrap());
        if primary {
            world.add_actor(Some(actor), false, &mut NoGrowth).unwrap();
        }
        let mut bytes = vec![0; 96];
        bytes[..4].copy_from_slice(&0x1631u32.to_le_bytes());
        bytes[8..12].copy_from_slice(&(pool_bytes as u32).to_le_bytes());
        bytes[12..16].copy_from_slice(&1u32.to_le_bytes());
        bytes.extend(0i32.to_le_bytes());
        bytes.extend(256i32.to_le_bytes());
        let mut descriptor = [0; 108];
        descriptor[..12].copy_from_slice(b"owned.frame\0");
        descriptor[36..40].copy_from_slice(b"rec\0");
        for (offset, value) in [
            (72, 8u32),
            (76, 44),
            (80, 56),
            (84, 56),
            (88, 55),
            (92, 1234),
            (96, 4),
            (100, pool_bytes as u32),
        ] {
            descriptor[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes.extend(descriptor);
        let mut pool = vec![0; pool_bytes];
        if pool_bytes >= 12 {
            pool[8..12].copy_from_slice(&123u32.to_le_bytes());
        }
        bytes.extend(pool);
        (world, model, actor, bytes)
    }
    #[test]
    fn cutscene_binding_uses_actual_primary_world_model_and_retained_record_owner() {
        use crate::cutscene_records::ActorUserData;
        use crate::frame_lookup::ModelFrameLookup;
        let (mut world, model, actor, bytes) = binding_fixture(true, 12);
        let mut owner = owner();
        owner
            .load_full_prefix_in_world(CutsceneSource::from_bytes(bytes), &world)
            .unwrap();
        owner.read_record_inputs(&mut world).unwrap();
        let pool = owner
            .record_inputs()
            .unwrap()
            .published_pool()
            .unwrap()
            .clone();
        assert!(owner
            .record_inputs()
            .unwrap()
            .buffer(crate::cutscene_records::CutsceneRecordInputKind::RecordPool)
            .is_none());
        assert_eq!(
            world.manager.records.pool(&pool).unwrap().allocated_bytes(),
            12
        );
        let bound = owner
            .bind_first_recorded_actor(&mut world, &ModelFrameLookup::borrow(model.arena(), true))
            .unwrap();
        assert_eq!(bound.actor, actor);
        assert_eq!(
            world.state(actor).unwrap().user_data,
            ActorUserData::Recorded(bound.record.clone())
        );
        assert_eq!(world.state(actor).unwrap().flags, 0x14);
        assert_eq!(world.manager.first, [actor]);
        assert_eq!(world.manager.associations.len(), 1);
        assert_eq!(
            world
                .manager
                .records
                .record(&bound.record)
                .unwrap()
                .playback_cursors()
                .0
                .unwrap()
                .byte_offset(),
            8
        );
        assert_eq!(owner.state(), 4);
        assert_eq!(owner.clocks(), (0, 0, 1.));
        world.manager.remove_association(actor, false).unwrap();
        assert_eq!(world.manager.records.retained_record_count(), 1);
        assert_eq!(
            world.state(actor).unwrap().user_data,
            ActorUserData::Recorded(bound.record)
        );
        assert!(owner.discard_prefix().manager_cleanup_required);
        assert_eq!(
            world.manager.records.pool(&pool).unwrap().allocated_bytes(),
            12
        );
    }
    #[test]
    fn cutscene_binding_does_not_substitute_static_source_or_retry_missing_actor() {
        use crate::frame_lookup::ModelFrameLookup;
        let (mut world, model, _, bytes) = binding_fixture(false, 12);
        let mut owner = owner();
        owner
            .load_full_prefix_in_world(CutsceneSource::from_bytes(bytes), &world)
            .unwrap();
        owner.read_record_inputs(&mut world).unwrap();
        assert_eq!(
            owner.bind_first_recorded_actor(
                &mut world,
                &ModelFrameLookup::borrow(model.arena(), true)
            ),
            Err(CutscenePrefixError::RequiredRecordedActor {
                frame_name: b"owned.frame".to_vec()
            })
        );
        assert_eq!(world.manager.records.retained_record_count(), 0);
        assert!(world.manager.associations.is_empty());
        assert_eq!(
            owner.bind_first_recorded_actor(
                &mut world,
                &ModelFrameLookup::borrow(model.arena(), true)
            ),
            Err(CutscenePrefixError::NotFresh)
        );
    }
    #[test]
    fn cutscene_binding_short_pool_forms_positions_without_reading_payload() {
        use crate::frame_lookup::ModelFrameLookup;
        for size in [1, 8, 12] {
            let (mut world, model, _, bytes) = binding_fixture(true, size);
            let mut owner = owner();
            owner
                .load_full_prefix_in_world(CutsceneSource::from_bytes(bytes), &world)
                .unwrap();
            owner.read_record_inputs(&mut world).unwrap();
            let bound = owner
                .bind_first_recorded_actor(
                    &mut world,
                    &ModelFrameLookup::borrow(model.arena(), true),
                )
                .unwrap();
            let data = world.manager.records.record(&bound.record).unwrap();
            let (current, previous) = data.playback_cursors();
            assert_eq!(current.unwrap().byte_offset(), 8);
            assert_eq!(previous.unwrap().byte_offset(), 8);
            assert_eq!(data.playback_current_item().unwrap(), current.unwrap());
            assert_eq!(data.playback_next_item().unwrap(), current.unwrap());
            assert_eq!(data.cursor_marker(), 0);
            // The12-byte case has payload123 at offset8. Native59a240 copies
            // the pointer value instead; the short cases prove no payload read.
            if size == 12 {
                let pool = owner.record_inputs().unwrap().published_pool().unwrap();
                assert_eq!(
                    &world
                        .manager
                        .records
                        .pool(pool)
                        .unwrap()
                        .initialized_prefix()[8..12],
                    &123u32.to_le_bytes()
                );
            }
            assert_eq!(
                owner.phase(),
                CutscenePrefixPhase::AfterFirstRecordedActorBinding
            );
        }
    }
    #[test]
    fn cutscene_binding_context_identity_survives_manager_transfer_guard() {
        let (mut first, _, _, bytes) = binding_fixture(true, 12);
        let mut second = empty_world();
        let mut owner = owner();
        owner
            .load_full_prefix_in_world(CutsceneSource::from_bytes(bytes), &first)
            .unwrap();
        std::mem::swap(&mut first.manager, &mut second.manager);
        assert_eq!(
            owner.read_record_inputs(&mut second),
            Err(CutscenePrefixError::ForeignManager)
        );
        assert_eq!(owner.source_position(), Some(104));
        assert!(owner.record_inputs().is_none());
    }

    #[test]
    fn cutscene_prefix_constructs_stopped_owner_and_stops_before_descriptors() {
        let mut owner = owner();
        assert_eq!(owner.state(), 4);
        assert_eq!(owner.previous_state(), 0);
        assert_eq!(owner.clocks(), (0, 0, 1.));
        assert_eq!(owner.seed().outer_allocator_byte, 0xa7);
        assert_eq!(owner.seed().manager_address_high_byte, 0x3c);
        assert_eq!(owner.constructor_collection_counts(&empty_world()), [0; 6]);
        assert_eq!(owner.resource_count(), Some(0));
        assert_eq!(owner.resource_growth(), Some(256));
        let bytes = recording();
        let length = bytes.len();
        let next = owner
            .load_full_prefix(CutsceneSource::from_bytes(bytes))
            .unwrap();
        assert_eq!(
            next,
            BeforeActorDescriptorAllocation {
                descriptor_count: 9
            }
        );
        assert_eq!(
            owner.phase(),
            CutscenePrefixPhase::BeforeActorDescriptorAllocation
        );
        assert_eq!(owner.source_position(), Some(208));
        assert_eq!(owner.state(), 4);
        assert_eq!(owner.clocks(), (0, 0, 1.));
        assert_eq!(owner.actor_counts(&empty_world()), (0, 0));
        assert_eq!(owner.resource_growth(), Some(713));
        assert_eq!(owner.resources().len(), 2);
        for entry in owner.resources() {
            assert_eq!(entry.id(), Some(42));
            assert_eq!(&entry.name_record().unwrap()[..7], b"input\0x");
            assert_eq!(entry.name_record().unwrap()[47], 0xfe);
            assert!(entry.binding_is_initialized_null());
        }
        // Automatic replay is unavailable; the retained first source survives.
        assert_eq!(
            owner.load_full_prefix(CutsceneSource::from_bytes(vec![])),
            Err(CutscenePrefixError::NotFresh)
        );
        assert_eq!(owner.source_position(), Some(208));
        assert_eq!(
            owner.discard_prefix(),
            CutscenePrefixDiscard {
                source_bytes_released: length,
                dictionary_slots_released: 2,
                manager_cleanup_required: false,
            }
        );
    }

    #[test]
    fn cutscene_prefix_header_and_word_failures_preserve_only_known_fields() {
        let full = recording();
        for length in 0..104 {
            let mut owner = owner();
            let result =
                owner.load_full_prefix(CutsceneSource::from_bytes(full[..length].to_vec()));
            assert!(matches!(result, Err(CutscenePrefixError::Read { .. })));
            assert_eq!(owner.phase(), CutscenePrefixPhase::Interrupted);
            assert_eq!(owner.source_position(), Some(length));
            assert_eq!(owner.header_prefix(), &full[..length.min(96)]);
            assert_eq!(owner.resources().len(), 0);
            if length <= 96 {
                assert_eq!(owner.resource_count(), Some(0));
            } else if length < 100 {
                assert_eq!(owner.resource_count(), None);
                assert_eq!(owner.pending_dictionary_word(), &full[96..length]);
            } else {
                assert_eq!(owner.resource_count(), Some(2));
                if length == 100 {
                    assert_eq!(owner.resource_growth(), Some(256));
                } else {
                    assert_eq!(owner.resource_growth(), None);
                    assert_eq!(owner.pending_dictionary_word(), &full[100..length]);
                }
            }
            assert_eq!(owner.state(), 4);
            owner.discard_prefix();
        }
    }

    #[test]
    fn cutscene_prefix_publishes_dictionary_before_entry_reads_and_retains_prefix() {
        let full = recording();
        for length in 104..208 {
            let mut owner = owner();
            assert!(matches!(
                owner.load_full_prefix(CutsceneSource::from_bytes(full[..length].to_vec())),
                Err(CutscenePrefixError::Read { .. })
            ));
            assert_eq!(owner.source_position(), Some(length));
            assert_eq!(owner.resources().len(), 2);
            let complete = (length - 104) / 52;
            for entry in &owner.resources()[..complete] {
                assert_eq!(entry.id(), Some(42));
                assert!(entry.binding_is_initialized_null());
            }
            let entry = &owner.resources()[complete];
            let in_entry = (length - 104) % 52;
            assert_eq!(entry.id(), (in_entry >= 4).then_some(42));
            assert_eq!(entry.name_prefix().len(), in_entry.saturating_sub(4));
            assert!(!entry.binding_is_initialized_null());
            let report = owner.discard_prefix();
            assert_eq!(report.dictionary_slots_released, 2);
            assert_eq!(report.source_bytes_released, length);
        }
    }

    #[test]
    fn cutscene_prefix_bad_magic_closes_buffered_source_without_game_side_effects() {
        let mut owner = owner();
        let mut bytes = header(999);
        bytes[..4].copy_from_slice(&123u32.to_le_bytes());
        assert_eq!(
            owner.load_full_prefix(CutsceneSource::from_bytes(bytes)),
            Err(CutscenePrefixError::BadMagic(123))
        );
        assert_eq!(owner.source_position(), None);
        assert!(owner.header().is_none());
        assert_eq!(owner.actor_counts(&empty_world()), (0, 0));
        assert_eq!(owner.resource_count(), Some(0));
        assert_eq!(owner.discard_prefix().source_bytes_released, 0);
    }

    #[test]
    fn cutscene_prefix_count_admission_occurs_after_both_metadata_reads() {
        for count in [-1i32, 65_537] {
            let mut bytes = header(3);
            bytes.extend(count.to_le_bytes());
            bytes.extend((-77i32).to_le_bytes());
            let mut owner = owner();
            assert_eq!(
                owner.load_full_prefix(CutsceneSource::from_bytes(bytes)),
                Err(CutscenePrefixError::ResourceCountOutsideDomain(count))
            );
            assert_eq!(owner.resource_count(), Some(count));
            assert_eq!(owner.resource_growth(), Some(-77));
            assert_eq!(owner.source_position(), Some(104));
            assert!(owner.resources().is_empty());
            owner.discard_prefix();
        }
        let mut bytes = header(0);
        bytes.extend(0i32.to_le_bytes());
        bytes.extend(0i32.to_le_bytes());
        let mut owner = owner();
        assert_eq!(
            owner
                .load_full_prefix(CutsceneSource::from_bytes(bytes))
                .unwrap()
                .descriptor_count,
            0
        );
        assert_eq!(owner.source_position(), Some(104));
        assert!(owner.resources().is_empty());
        owner.discard_prefix();
    }
}
