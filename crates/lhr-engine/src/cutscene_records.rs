//! Serialized LoadFull allocations and shared recorded-data backing.
//! Game SHA256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27,
//! RVA19a48d..19a57a. These buffers are file-format data, not game objects.
#![forbid(unsafe_code)]

use std::rc::Rc;

use crate::script_scheduler::ActorReferenceId;

use crate::cutscene_source::{CutsceneSource, CutsceneSourceHeader, CutsceneSourceReadError};

/// A host allocation bound, not an original file-format limit.
const MAX_BUFFER_BYTES: u32 = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CutsceneRecordInputKind {
    ActorDescriptors,
    RecordPool,
    CameraRecords,
    AuxiliaryA,
    AuxiliaryB,
}
#[derive(Debug, PartialEq, Eq)]
pub enum CutsceneRecordInputError {
    AllocationDomain {
        kind: CutsceneRecordInputKind,
        bytes: u64,
    },
    HostAllocation {
        kind: CutsceneRecordInputKind,
    },
    Read {
        kind: CutsceneRecordInputKind,
        error: CutsceneSourceReadError,
    },
    /// Zero-length allocation identity is outside this owner's current domain.
    EmptyRecordPool,
}

/// Only `initialized_prefix` has a source-established value. Reserved bytes
/// beyond it are inaccessible and do not model native uninitialized memory.
#[derive(Debug)]
pub struct CutsceneSerializedBuffer {
    bytes: Vec<u8>,
    initialized: usize,
}
impl CutsceneSerializedBuffer {
    fn allocate(
        kind: CutsceneRecordInputKind,
        bytes: u64,
    ) -> Result<Self, CutsceneRecordInputError> {
        if bytes > u64::from(MAX_BUFFER_BYTES) {
            return Err(CutsceneRecordInputError::AllocationDomain { kind, bytes });
        }
        let mut data = Vec::new();
        data.try_reserve_exact(bytes as usize)
            .map_err(|_| CutsceneRecordInputError::HostAllocation { kind })?;
        data.resize(bytes as usize, 0);
        Ok(Self {
            bytes: data,
            initialized: 0,
        })
    }
    fn read(
        &mut self,
        source: &mut CutsceneSource,
        kind: CutsceneRecordInputKind,
    ) -> Result<(), CutsceneRecordInputError> {
        let before = source.position();
        let result = source.read_exact(&mut self.bytes);
        self.initialized = source.position() - before;
        result.map_err(|error| CutsceneRecordInputError::Read { kind, error })
    }
    pub fn allocated_bytes(&self) -> usize {
        self.bytes.len()
    }
    pub fn initialized_prefix(&self) -> &[u8] {
        &self.bytes[..self.initialized]
    }
    pub fn is_complete(&self) -> bool {
        self.initialized == self.bytes.len()
    }
}

/// Live view of one complete serialized 108-byte descriptor. Its recorded
/// actor type is a separate format domain from mission source factory kinds.
#[derive(Debug)]
pub struct CutsceneActorDescriptor<'a> {
    bytes: &'a [u8; 108],
}
impl CutsceneActorDescriptor<'_> {
    pub fn frame_name_record(&self) -> &[u8; 36] {
        self.bytes[..36].try_into().unwrap()
    }
    pub fn recording_name_record(&self) -> &[u8; 36] {
        self.bytes[36..72].try_into().unwrap()
    }
    pub fn record_type_sizes(&self) -> [u32; 4] {
        [self.word(72), self.word(76), self.word(80), self.word(84)]
    }
    pub fn recorded_actor_type(&self) -> u32 {
        self.word(96)
    }
    pub fn record_byte_count(&self) -> u32 {
        self.word(100)
    }
    fn word(&self, offset: usize) -> u32 {
        u32::from_le_bytes(self.bytes[offset..offset + 4].try_into().unwrap())
    }
}

/// Published allocations are retained on an interrupted read. The owner pool
/// and manager pool alias exactly one allocation; neither copies its contents.
#[derive(Debug, Default)]
pub struct CutsceneRecordInputs {
    descriptors: Option<CutsceneSerializedBuffer>,
    pool: Option<CutsceneSerializedBuffer>,
    camera: Option<CutsceneSerializedBuffer>,
    auxiliary_a: Option<CutsceneSerializedBuffer>,
    auxiliary_b: Option<CutsceneSerializedBuffer>,
    published_pool: Option<RecordPoolId>,
}
impl CutsceneRecordInputs {
    pub fn buffer(&self, kind: CutsceneRecordInputKind) -> Option<&CutsceneSerializedBuffer> {
        match kind {
            CutsceneRecordInputKind::ActorDescriptors => self.descriptors.as_ref(),
            CutsceneRecordInputKind::RecordPool => self.pool.as_ref(),
            CutsceneRecordInputKind::CameraRecords => self.camera.as_ref(),
            CutsceneRecordInputKind::AuxiliaryA => self.auxiliary_a.as_ref(),
            CutsceneRecordInputKind::AuxiliaryB => self.auxiliary_b.as_ref(),
        }
    }
    /// Does not inspect records or names before their complete table read.
    pub fn descriptor(&self, index: usize) -> Option<CutsceneActorDescriptor<'_>> {
        let table = self.descriptors.as_ref()?;
        if !table.is_complete() {
            return None;
        }
        let start = index.checked_mul(108)?;
        let end = start.checked_add(108)?;
        Some(CutsceneActorDescriptor {
            bytes: table
                .initialized_prefix()
                .get(start..end)?
                .try_into()
                .ok()?,
        })
    }
    pub fn published_pool(&self) -> Option<&RecordPoolId> {
        self.published_pool.as_ref()
    }
    pub fn record_pool<'a>(
        &'a self,
        arena: &'a CutsceneRecordArena,
    ) -> Result<Option<&'a CutsceneSerializedBuffer>, CutsceneRecordError> {
        match &self.published_pool {
            Some(id) => arena.pool(id).map(Some),
            None => Ok(self.pool.as_ref()),
        }
    }
    pub(crate) fn read_allocated_inputs(
        &mut self,
        source: &mut CutsceneSource,
        header: &CutsceneSourceHeader,
    ) -> Result<(), CutsceneRecordInputError> {
        use CutsceneRecordInputKind::*;
        // Each bound is checked only at its reached allocation, preserving the
        // preceding publication prefix. Wide arithmetic rejects overflow instead
        // of admitting x86 wrapping allocation sizes as safe file capacities.
        self.descriptors = Some(CutsceneSerializedBuffer::allocate(
            ActorDescriptors,
            u64::from(header.word(12)) * 108,
        )?);
        let pool_bytes = header.word(8);
        if pool_bytes == 0 {
            return Err(CutsceneRecordInputError::EmptyRecordPool);
        }
        self.pool = Some(CutsceneSerializedBuffer::allocate(
            RecordPool,
            u64::from(pool_bytes),
        )?);
        let camera_bytes = u64::from(header.word(20)) * 64 + u64::from(header.word(24)) * 56;
        if camera_bytes != 0 {
            self.camera = Some(CutsceneSerializedBuffer::allocate(
                CameraRecords,
                camera_bytes,
            )?);
        }
        if header.word(28) != 0 {
            self.auxiliary_a = Some(CutsceneSerializedBuffer::allocate(
                AuxiliaryA,
                u64::from(header.word(28)),
            )?);
        }
        if header.word(32) != 0 {
            self.auxiliary_b = Some(CutsceneSerializedBuffer::allocate(
                AuxiliaryB,
                u64::from(header.word(32)),
            )?);
        }
        self.descriptors
            .as_mut()
            .unwrap()
            .read(source, ActorDescriptors)?;
        // The alias is published only AFTER all five ordered source reads and
        // input close; therefore this allocation remains uniquely writable here.
        self.pool.as_mut().unwrap().read(source, RecordPool)?;
        if let Some(data) = &mut self.camera {
            data.read(source, CameraRecords)?;
        }
        if let Some(data) = &mut self.auxiliary_a {
            data.read(source, AuxiliaryA)?;
        }
        if let Some(data) = &mut self.auxiliary_b {
            data.read(source, AuxiliaryB)?;
        }
        Ok(())
    }
    /// Moves the allocation into the actual ActorWorld manager. The source
    /// owner retains only its nonowning pool identity (native owner+ec alias).
    pub(crate) fn publish_manager_pool(
        &mut self,
        arena: &mut CutsceneRecordArena,
    ) -> Result<RecordPoolId, CutsceneRecordError> {
        if arena.has_pool() {
            return Err(CutsceneRecordError::PoolAlreadyPublished);
        }
        arena
            .generation
            .checked_add(1)
            .ok_or(CutsceneRecordError::PoolGenerationExhausted)?;
        let pool = self
            .pool
            .take()
            .ok_or(CutsceneRecordError::UnavailablePool)?;
        let id = arena.install_pool(pool)?;
        self.published_pool = Some(id.clone());
        Ok(id)
    }
}

// Identities retain only a tiny scope token, never record/pool storage. Slots
// are never reused; each pool generation changes on publication/invalidation.
#[derive(Clone, Debug)]
pub struct RecordManagerId(Rc<()>);
impl PartialEq for RecordManagerId {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for RecordManagerId {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordedActorId {
    manager: RecordManagerId,
    slot: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordPoolId {
    manager: RecordManagerId,
    generation: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordPoolPosition {
    pool: RecordPoolId,
    byte_offset: usize,
}

/// Actual C_actor+8 storage. Opaque legacy values remain explicit and cannot
/// be interpreted as a live recorded-data allocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActorUserData {
    Null,
    Recorded(RecordedActorId),
    OpaqueLegacy(u32),
}

/// One ordinary association in the SAME ActorWorld.manager vector. The two
/// names are inline native strings represented as bounded semantic strings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordedActorAssociation {
    pub uses_record_pool: bool,
    pub recorded_actor_type: u32,
    pub frame_name: Vec<u8>,
    pub recording_name: Vec<u8>,
    pub actor: Option<ActorReferenceId>,
    pub record: Option<RecordedActorId>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CutsceneRecordError {
    ForeignRecord,
    MissingRecord,
    UnavailablePool,
    PoolAlreadyPublished,
    PoolGenerationExhausted,
    HostAllocation,
    PoolPositionOutsideDomain,
    UnterminatedRecordingName { record: RecordedActorId },
}

/// Working descriptive names for the constructor/59a240 projection only.
/// Unknown final ordinary word88 has no fabricated initialized value here.
#[derive(Debug)]
pub struct RecordedActorData {
    recording_name: [u8; 36],
    recording_name_len: usize,
    recording_name_complete: bool,
    type_sizes: [u32; 4],
    control_word: u32, // descriptor92 -> native record20
    playback_current_cursor: Option<RecordPoolPosition>, // native record0
    playback_previous_cursor: Option<RecordPoolPosition>, // native record4
    playback_current_item: Option<RecordPoolPosition>, // native record8
    playback_saved_item: Option<RecordPoolPosition>, // native record12
    playback_next_item: Option<RecordPoolPosition>, // native record16
    first_block: Option<RecordPoolPosition>, // native record24
    tail_block: Option<RecordPoolPosition>, // native record28
    cursor_marker: u32, // descriptor88 -> record84, then59a240 clears it
}
impl RecordedActorData {
    pub fn recording_name(&self) -> Option<&[u8]> {
        self.recording_name_complete
            .then_some(&self.recording_name[..self.recording_name_len])
    }
    pub fn recording_name_prefix(&self) -> &[u8] {
        &self.recording_name[..self.recording_name_len]
    }
    pub fn record_type_sizes(&self) -> [u32; 4] {
        self.type_sizes
    }
    pub fn control_word(&self) -> u32 {
        self.control_word
    }
    pub fn playback_cursors(&self) -> (Option<&RecordPoolPosition>, Option<&RecordPoolPosition>) {
        (
            self.playback_current_cursor.as_ref(),
            self.playback_previous_cursor.as_ref(),
        )
    }
    pub fn playback_current_item(&self) -> Option<&RecordPoolPosition> {
        self.playback_current_item.as_ref()
    }
    pub fn playback_next_item(&self) -> Option<&RecordPoolPosition> {
        self.playback_next_item.as_ref()
    }
    pub fn playback_saved_item(&self) -> Option<&RecordPoolPosition> {
        self.playback_saved_item.as_ref()
    }
    pub fn cursor_marker(&self) -> u32 {
        self.cursor_marker
    }
    pub fn first_block(&self) -> Option<&RecordPoolPosition> {
        self.first_block.as_ref()
    }
    pub fn tail_block(&self) -> Option<&RecordPoolPosition> {
        self.tail_block.as_ref()
    }
}
impl RecordPoolPosition {
    pub fn byte_offset(&self) -> usize {
        self.byte_offset
    }
    pub fn pool(&self) -> &RecordPoolId {
        &self.pool
    }
}

/// Retained allocations owned by the actual ActorWorld manager. POD association
/// erasure does not release record objects (native59d1f0(false)). A failed append
/// likewise leaves its already-produced object here, independently of actor or
/// association links. No native Clear/Close or record-object free is supplied.
#[derive(Debug)]
pub struct CutsceneRecordArena {
    identity: RecordManagerId,
    generation: u64,
    pool: Option<CutsceneSerializedBuffer>,
    records: Vec<RecordedActorData>,
}
impl Default for CutsceneRecordArena {
    fn default() -> Self {
        Self {
            identity: RecordManagerId(Rc::new(())),
            generation: 0,
            pool: None,
            records: Vec::new(),
        }
    }
}
impl CutsceneRecordArena {
    pub fn identity(&self) -> RecordManagerId {
        self.identity.clone()
    }
    pub fn is_fresh(&self) -> bool {
        self.pool.is_none() && self.records.is_empty()
    }
    pub fn has_pool(&self) -> bool {
        self.pool.is_some()
    }
    pub fn retained_record_count(&self) -> usize {
        self.records.len()
    }
    pub fn pool(
        &self,
        id: &RecordPoolId,
    ) -> Result<&CutsceneSerializedBuffer, CutsceneRecordError> {
        if id.manager != self.identity || id.generation != self.generation {
            return Err(CutsceneRecordError::UnavailablePool);
        }
        self.pool
            .as_ref()
            .ok_or(CutsceneRecordError::UnavailablePool)
    }
    fn install_pool(
        &mut self,
        pool: CutsceneSerializedBuffer,
    ) -> Result<RecordPoolId, CutsceneRecordError> {
        if self.pool.is_some() {
            return Err(CutsceneRecordError::PoolAlreadyPublished);
        }
        let generation = self
            .generation
            .checked_add(1)
            .ok_or(CutsceneRecordError::PoolGenerationExhausted)?;
        self.pool = Some(pool);
        self.generation = generation;
        Ok(RecordPoolId {
            manager: self.identity.clone(),
            generation,
        })
    }
    pub fn record(&self, id: &RecordedActorId) -> Result<&RecordedActorData, CutsceneRecordError> {
        if id.manager != self.identity {
            return Err(CutsceneRecordError::ForeignRecord);
        }
        self.records
            .get(id.slot)
            .ok_or(CutsceneRecordError::MissingRecord)
    }
    fn record_mut(
        &mut self,
        id: &RecordedActorId,
    ) -> Result<&mut RecordedActorData, CutsceneRecordError> {
        if id.manager != self.identity {
            return Err(CutsceneRecordError::ForeignRecord);
        }
        self.records
            .get_mut(id.slot)
            .ok_or(CutsceneRecordError::MissingRecord)
    }
    /// LoadFull's record allocation/field stores, BEFORE59d740 registration.
    /// Retains the allocated projection even if the inline name exceeds the
    /// bounded host domain, preserving the last initialized producer prefix.
    pub fn allocate_record(
        &mut self,
        descriptor: &CutsceneActorDescriptor<'_>,
        pool: &RecordPoolId,
        byte_offset: usize,
    ) -> Result<RecordedActorId, CutsceneRecordError> {
        self.pool(pool)?;
        self.records
            .try_reserve(1)
            .map_err(|_| CutsceneRecordError::HostAllocation)?;
        let id = RecordedActorId {
            manager: self.identity.clone(),
            slot: self.records.len(),
        };
        let block = RecordPoolPosition {
            pool: pool.clone(),
            byte_offset,
        };
        self.records.push(RecordedActorData {
            recording_name: [0; 36],
            recording_name_len: 0,
            recording_name_complete: false,
            type_sizes: descriptor.record_type_sizes(),
            control_word: descriptor.word(92),
            playback_current_cursor: None,
            playback_previous_cursor: None,
            playback_current_item: None,
            playback_saved_item: None,
            playback_next_item: None,
            first_block: Some(block.clone()),
            tail_block: Some(block),
            cursor_marker: descriptor.word(88),
        });
        let data = self.records.last_mut().unwrap();
        for &byte in descriptor.recording_name_record() {
            if byte == 0 {
                data.recording_name_complete = true;
                return Ok(id);
            }
            data.recording_name[data.recording_name_len] = byte;
            data.recording_name_len += 1;
        }
        Err(CutsceneRecordError::UnterminatedRecordingName { record: id })
    }
    /// Actual59a240 only copies pool-relative pointer values; it does NOT read
    /// the pool payload. Positions at/past the backing end are valid to form;
    /// a future reader must separately validate liveness and bounds.
    pub fn initialize_actor_playback(
        &mut self,
        id: &RecordedActorId,
    ) -> Result<(), CutsceneRecordError> {
        let mut item = self.record(id)?.first_block.clone();
        if let Some(position) = &mut item {
            position.byte_offset = position
                .byte_offset
                .checked_add(8)
                .ok_or(CutsceneRecordError::PoolPositionOutsideDomain)?;
        }
        let data = self.record_mut(id)?;
        data.playback_next_item = item;
        data.playback_current_item = data.playback_next_item.clone();
        data.playback_current_cursor = data.playback_current_item.clone();
        data.playback_previous_cursor = data.playback_current_cursor.clone();
        data.cursor_marker = 0;
        Ok(())
    }
    /// Explicit native5c25e0 bounded effect: clears first-block link only. It
    /// does not free this object, pool, tail link or playback cursor fields.
    pub fn reset_first_block(&mut self, id: &RecordedActorId) -> Result<(), CutsceneRecordError> {
        self.record_mut(id)?.first_block = None;
        Ok(())
    }
    /// Explicit terminal host pool disposal, never native Clear/Close. Record
    /// objects remain retained and their old pool links become unavailable.
    pub fn discard_host_pool(&mut self) -> usize {
        self.pool.take().map_or(0, |pool| pool.allocated_bytes())
    }
    /// Terminal host cleanup only. Callers must separately establish that live
    /// actors/associations no longer use this arena; no native Close is implied.
    pub fn discard_host_records(self) -> (usize, usize) {
        (
            self.records.len(),
            self.pool
                .as_ref()
                .map_or(0, CutsceneSerializedBuffer::allocated_bytes),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (CutsceneSourceHeader, Vec<u8>) {
        let mut header = [0u8; 96];
        for (offset, value) in [(8, 4u32), (12, 2), (20, 1), (28, 3), (32, 2)] {
            header[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        let bytes = (0..289).map(|i| (i % 251) as u8).collect();
        (CutsceneSourceHeader::from_bytes(header), bytes)
    }
    #[test]
    fn cutscene_record_inputs_keep_every_partial_read_and_publish_alias_only_after_close() {
        use CutsceneRecordInputKind::*;
        let (header, bytes) = fixture();
        for cut in 0..bytes.len() {
            let mut owner = CutsceneRecordInputs::default();
            let mut source = CutsceneSource::from_bytes(bytes[..cut].to_vec());
            let error = owner
                .read_allocated_inputs(&mut source, &header)
                .unwrap_err();
            let expected = if cut < 216 {
                ActorDescriptors
            } else if cut < 220 {
                RecordPool
            } else if cut < 284 {
                CameraRecords
            } else if cut < 287 {
                AuxiliaryA
            } else {
                AuxiliaryB
            };
            assert!(
                matches!(error, CutsceneRecordInputError::Read { kind, .. } if kind == expected)
            );
            assert_eq!(source.position(), cut);
            assert_eq!(owner.descriptor(0).is_some(), cut >= 216);
            assert!(owner.published_pool().is_none());
            let mut consumed = 0;
            for kind in [
                ActorDescriptors,
                RecordPool,
                CameraRecords,
                AuxiliaryA,
                AuxiliaryB,
            ] {
                let buffer = owner.buffer(kind).unwrap();
                let initialized = cut.saturating_sub(consumed).min(buffer.allocated_bytes());
                assert_eq!(
                    buffer.initialized_prefix(),
                    &bytes[consumed..consumed + initialized]
                );
                consumed += buffer.allocated_bytes();
            }
        }
    }
    #[test]
    fn cutscene_record_inputs_share_one_pool_and_do_not_consume_trailing_data() {
        let (header, mut bytes) = fixture();
        bytes.extend([8, 9]);
        let mut source = CutsceneSource::from_bytes(bytes);
        let mut owner = CutsceneRecordInputs::default();
        owner.read_allocated_inputs(&mut source, &header).unwrap();
        assert_eq!(source.position(), 289);
        assert!(owner.published_pool().is_none());
        let mut arena = CutsceneRecordArena::default();
        let id = owner.publish_manager_pool(&mut arena).unwrap();
        assert_eq!(owner.published_pool(), Some(&id));
        assert!(owner.buffer(CutsceneRecordInputKind::RecordPool).is_none());
        assert_eq!(
            owner
                .record_pool(&arena)
                .unwrap()
                .unwrap()
                .allocated_bytes(),
            4
        );
    }
    #[test]
    fn cutscene_record_inputs_reached_allocation_guard_keeps_previous_publications() {
        use CutsceneRecordInputKind::*;
        let (mut header, bytes) = fixture();
        let mut serialized = *header.serialized_bytes();
        serialized[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
        header = CutsceneSourceHeader::from_bytes(serialized);
        let mut source = CutsceneSource::from_bytes(bytes);
        let mut owner = CutsceneRecordInputs::default();
        assert!(matches!(
            owner.read_allocated_inputs(&mut source, &header),
            Err(CutsceneRecordInputError::AllocationDomain {
                kind: AuxiliaryA,
                ..
            })
        ));
        assert!(owner.buffer(ActorDescriptors).is_some());
        assert!(owner.buffer(RecordPool).is_some());
        assert!(owner.buffer(CameraRecords).is_some());
        assert!(owner.buffer(AuxiliaryA).is_none());
        assert!(owner.buffer(AuxiliaryB).is_none());
        assert_eq!(source.position(), 0);
    }
    fn arena_record_fixture() -> (CutsceneRecordInputs, CutsceneRecordArena, RecordPoolId) {
        let mut header = [0; 96];
        header[8..12].copy_from_slice(&12u32.to_le_bytes());
        header[12..16].copy_from_slice(&1u32.to_le_bytes());
        let mut bytes = vec![0; 120];
        bytes[36..40].copy_from_slice(b"rec\0");
        bytes[88..92].copy_from_slice(&55u32.to_le_bytes());
        bytes[116..120].copy_from_slice(&7u32.to_le_bytes());
        let mut inputs = CutsceneRecordInputs::default();
        inputs
            .read_allocated_inputs(
                &mut CutsceneSource::from_bytes(bytes),
                &CutsceneSourceHeader::from_bytes(header),
            )
            .unwrap();
        let mut arena = CutsceneRecordArena::default();
        let pool = inputs.publish_manager_pool(&mut arena).unwrap();
        (inputs, arena, pool)
    }
    #[test]
    fn cutscene_record_arena_rejects_foreign_ids_and_invalidates_nonowning_pool_links() {
        let (inputs, mut arena, pool) = arena_record_fixture();
        let record = arena
            .allocate_record(&inputs.descriptor(0).unwrap(), &pool, 0)
            .unwrap();
        let other = CutsceneRecordArena::default();
        assert_eq!(
            other.record(&record).unwrap_err(),
            CutsceneRecordError::ForeignRecord
        );
        assert_eq!(
            other.pool(&pool).unwrap_err(),
            CutsceneRecordError::UnavailablePool
        );
        assert_eq!(arena.discard_host_pool(), 12);
        assert_eq!(arena.retained_record_count(), 1);
        arena.initialize_actor_playback(&record).unwrap();
        let data = arena.record(&record).unwrap();
        assert_eq!(data.playback_current_item().unwrap().byte_offset(), 8);
        assert_eq!(data.playback_cursors().0.unwrap().byte_offset(), 8);
        assert_eq!(data.playback_cursors().1.unwrap().byte_offset(), 8);
        assert_eq!(data.cursor_marker(), 0);
        let new_pool = arena
            .install_pool(
                CutsceneSerializedBuffer::allocate(CutsceneRecordInputKind::RecordPool, 12)
                    .unwrap(),
            )
            .unwrap();
        assert_ne!(new_pool, pool);
        assert_eq!(
            arena.pool(&pool).unwrap_err(),
            CutsceneRecordError::UnavailablePool
        );
    }
    #[test]
    fn cutscene_record_arena_retains_unterminated_record_allocation_and_reset_does_not_free() {
        let (mut inputs, mut arena, pool) = arena_record_fixture();
        inputs.descriptors.as_mut().unwrap().bytes[36..72].fill(0x5a);
        let error = arena
            .allocate_record(&inputs.descriptor(0).unwrap(), &pool, 0)
            .unwrap_err();
        let CutsceneRecordError::UnterminatedRecordingName { record } = error else {
            panic!("name guard")
        };
        assert_eq!(arena.retained_record_count(), 1);
        assert_eq!(
            arena.record(&record).unwrap().recording_name_prefix(),
            [0x5a; 36]
        );
        arena.reset_first_block(&record).unwrap();
        let data = arena.record(&record).unwrap();
        assert!(data.first_block().is_none());
        assert!(data.tail_block().is_some());
        assert_eq!(arena.pool(&pool).unwrap().allocated_bytes(), 12);
    }
    #[test]
    fn cutscene_record_reset_forms_positions_with_empty_backing_and_null_block_without_reads() {
        let (inputs, _, _) = arena_record_fixture();
        let mut arena = CutsceneRecordArena::default();
        let pool = arena
            .install_pool(
                CutsceneSerializedBuffer::allocate(CutsceneRecordInputKind::RecordPool, 0).unwrap(),
            )
            .unwrap();
        let id = arena
            .allocate_record(&inputs.descriptor(0).unwrap(), &pool, 0)
            .unwrap();
        arena.initialize_actor_playback(&id).unwrap();
        let data = arena.record(&id).unwrap();
        assert_eq!(data.playback_cursors().0.unwrap().byte_offset(), 8);
        assert_eq!(data.playback_cursors().0, data.playback_cursors().1);
        assert_eq!(arena.pool(&pool).unwrap().allocated_bytes(), 0);
        arena.reset_first_block(&id).unwrap();
        arena.initialize_actor_playback(&id).unwrap();
        let data = arena.record(&id).unwrap();
        assert_eq!(data.playback_cursors(), (None, None));
        assert!(data.playback_current_item().is_none());
        assert!(data.playback_next_item().is_none());
        assert!(data.tail_block().is_some());
    }
}
