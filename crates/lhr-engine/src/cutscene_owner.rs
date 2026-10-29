//! Bounded live PC cutscene owner and LoadFull resource-dictionary prefix.
//! Game SHA256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27:
//! constructor RVA19cc40, resource constructor19b5f0, manager constructor19cf70,
//! dictionary reader19bab0, LoadFull19a3c0. No playback or native Close is supplied.
#![forbid(unsafe_code)]

use crate::cutscene_source::{CutsceneSource, CutsceneSourceHeader, CutsceneSourceReadError};
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
}

/// Required recorded-actor pipeline, never LoadFull success or playing.
/// This prefix stops BEFORE descriptor allocation, not at an actor binding call.
#[derive(Debug, PartialEq, Eq)]
pub struct BeforeActorDescriptorAllocation {
    /// The next native phase allocates the serialized actor-descriptor table.
    pub descriptor_count: u32,
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

/// Actual empty actor collections are owned here, not mirrored through numeric
/// addresses. Their population/record links remain a required later dependency.
#[derive(Debug, Default)]
struct CutsceneActors {
    primary: Vec<ActorReferenceId>,
    secondary: Vec<ActorReferenceId>,
}

// Constructor-only element domains. No populated value is admitted until its
// actual producer/ownership contract exists; these vectors genuinely start empty.
#[derive(Debug)]
enum RetainedCutsceneObject {}
#[derive(Debug)]
enum RecordedActorLink {}
#[derive(Debug)]
enum RecordedAuxiliaryEntry {}

/// Constructor-established part of the original singleton. Fields outside
/// this projection stay unavailable; no full-object layout/function credit.
#[derive(Debug)]
pub struct CutsceneOwner {
    seed: CutsceneConstructorSeed,
    actors: CutsceneActors,
    auxiliary_words: Vec<u32>,
    retained_objects: Vec<RetainedCutsceneObject>,
    manager_actors: Vec<RecordedActorLink>,
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
}

#[derive(Debug, PartialEq, Eq)]
pub struct CutscenePrefixDiscard {
    pub source_bytes_released: usize,
    pub dictionary_slots_released: usize,
}

impl CutsceneOwner {
    pub fn constructed(seed: CutsceneConstructorSeed) -> Self {
        Self {
            seed,
            actors: CutsceneActors::default(),
            auxiliary_words: Vec::new(),
            retained_objects: Vec::new(),
            manager_actors: Vec::new(),
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
    pub fn actor_counts(&self) -> (usize, usize) {
        (self.actors.primary.len(), self.actors.secondary.len())
    }
    pub fn constructor_collection_counts(&self) -> [usize; 6] {
        [
            self.actors.primary.len(),
            self.actors.secondary.len(),
            self.auxiliary_words.len(),
            self.retained_objects.len(),
            self.manager_actors.len(),
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
        self.source.as_ref().map(CutsceneSource::position)
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
    /// The unopened actor-record stage owns no record blob or aliased link.
    pub fn discard_prefix(self) -> CutscenePrefixDiscard {
        CutscenePrefixDiscard {
            source_bytes_released: self.source.as_ref().map_or(0, CutsceneSource::source_len),
            dictionary_slots_released: self.resources.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn cutscene_prefix_constructs_stopped_owner_and_stops_before_descriptors() {
        let mut owner = owner();
        assert_eq!(owner.state(), 4);
        assert_eq!(owner.previous_state(), 0);
        assert_eq!(owner.clocks(), (0, 0, 1.));
        assert_eq!(owner.seed().outer_allocator_byte, 0xa7);
        assert_eq!(owner.seed().manager_address_high_byte, 0x3c);
        assert_eq!(owner.constructor_collection_counts(), [0; 6]);
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
        assert_eq!(owner.actor_counts(), (0, 0));
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
                dictionary_slots_released: 2
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
        assert_eq!(owner.actor_counts(), (0, 0));
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
