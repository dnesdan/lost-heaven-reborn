//! Bounded PC managed-texture removal, descriptive names for PC-only globals.
//! Original LS3DF.dll SHA-256:
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! RVA0x7aee0: reverse first-record match; conditional reverse256 queue clear;
//! filename delete, record delete, swap removal; three pools' reverse owner
//! clears; flags &=0x3fffffff; leave critical section; AL1. No worker, Request,
//! Create, pool shutdown or graphics allocation/release is reconstructed here.
//! Snapshot intake/typed identities and failure handling are host adapters.
use crate::itexture::{ManagedRecordKey, TextureGraphics, TextureKey, TextureSurface};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ManagedFilenameAllocation(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RecordId {
    arena: u64,
    slot: u32,
}
impl RecordId {
    pub fn arena(self) -> u64 {
        self.arena
    }
    pub fn slot(self) -> u32 {
        self.slot
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PoolSlotId {
    arena: u64,
    pool: u8,
    slot: u32,
}
impl PoolSlotId {
    pub fn arena(self) -> u64 {
        self.arena
    }
    pub fn pool(self) -> u8 {
        self.pool
    }
    pub fn slot(self) -> u32 {
        self.slot
    }
}
/// Required effects, not successful substitutes for the native manager body.
/// Enter Err MUST leave no lock held. After successful enter, the host attempts
/// leave exactly once even on destructive failure. Leave Err means lock state
/// is unknown; no automatic retry. This is availability policy, not native
/// exception/worker recovery. Callbacks are serialized/nonreentrant.
pub trait ManagedTextureEffects {
    type Error;
    fn enter_texture_section(&mut self) -> Result<(), Self::Error>;
    fn leave_texture_section(&mut self) -> Result<(), Self::Error>;
    /// The original delete is unconditional, including the null pointer case.
    fn delete_managed_filename(
        &mut self,
        allocation: Option<ManagedFilenameAllocation>,
    ) -> Result<(), Self::Error>;
    fn delete_managed_record(&mut self, allocation: ManagedRecordKey) -> Result<(), Self::Error>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CriticalState {
    NotHeld,
    Held,
    LeaveFailed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordPhase {
    Live,
    FreeingFilename,
    FilenameFreed,
    FreeingRecord,
    RecordFreed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManagedStage {
    Enter,
    Filename,
    Record,
    Leave,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManagerError {
    Poisoned,
    AlreadySeeded,
    ForeignWorld,
    UnknownRecord,
    UnknownPool,
    InvalidIdentity,
    InvalidCount,
    Incoherent,
    HostAllocation,
    IdentityOverflow,
}
#[derive(Debug, PartialEq, Eq)]
pub enum ManagedOperationError<E> {
    State(ManagerError),
    Backend {
        stage: ManagedStage,
        error: E,
        leave_error: Option<E>,
    },
}
impl<E> From<ManagerError> for ManagedOperationError<E> {
    fn from(e: ManagerError) -> Self {
        Self::State(e)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueueCounters {
    pub out: u32,
    pub put: u32,
    pub len: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoolResources {
    /// Actual pool-owned references, independent of the weak texture owner.
    /// Their association with the opaque native payload is host intake metadata;
    /// 7aee0 itself accesses only owner+0 and the0x2c stride.
    pub graphics: Option<TextureGraphics>,
    pub surface: Option<TextureSurface>,
}
pub struct RecordIntake {
    pub owner: TextureKey,
    pub allocation: ManagedRecordKey,
    pub file_name: Option<ManagedFilenameAllocation>,
    pub preserved: Vec<u8>,
}
pub struct PoolIntake {
    pub owner: Option<TextureKey>,
    pub preserved: [u8; 40],
    pub resources: PoolResources,
}
#[derive(Clone, Copy, Debug)]
pub struct ManagedBinding {
    pub texture: TextureKey,
    pub flags: u32,
    pub record: Option<ManagedRecordKey>,
    pub graphics: Option<TextureGraphics>,
}
/// A serialized seeded world, not AddTextureToTextureManager/Restore/Request.
/// All supplied allocator and COM references remain caller-owned on admission
/// error; successful admission adopts them. No callbacks occur during intake.
pub struct ManagedWorldIntake {
    pub records: Vec<RecordIntake>,
    pub queue: [Option<TextureKey>; 256],
    pub counters: QueueCounters,
    pub pools: [Vec<PoolIntake>; 3],
    pub bindings: Vec<ManagedBinding>,
}
#[derive(Debug, PartialEq, Eq)]
pub struct ManagedRecord {
    pub id: RecordId,
    pub owner: TextureKey,
    pub allocation: ManagedRecordKey,
    pub file_name: Option<ManagedFilenameAllocation>,
    pub preserved: Vec<u8>,
    pub phase: RecordPhase,
}
#[derive(Debug, PartialEq, Eq)]
pub struct PoolSlot {
    pub id: PoolSlotId,
    pub owner: Option<TextureKey>,
    pub preserved: [u8; 40],
    pub resources: PoolResources,
}
/// Private mutation, immutable diagnostics. Pool-owned resources survive owner
/// clears and require a separately reconstructed shutdown; Drop cannot free them.
pub struct TextureManager {
    identity: u64,
    seeded: bool,
    records: Vec<ManagedRecord>,
    active: Vec<RecordId>,
    queue: [Option<TextureKey>; 256],
    counters: QueueCounters,
    pools: [Vec<PoolSlot>; 3],
    critical: CriticalState,
    poisoned: bool,
}
impl TextureManager {
    pub(crate) fn new(identity: u64) -> Self {
        Self {
            identity,
            seeded: false,
            records: Vec::new(),
            active: Vec::new(),
            queue: [None; 256],
            counters: QueueCounters::default(),
            pools: [Vec::new(), Vec::new(), Vec::new()],
            critical: CriticalState::NotHeld,
            poisoned: false,
        }
    }
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }
    pub fn is_seeded(&self) -> bool {
        self.seeded
    }
    pub fn critical_state(&self) -> CriticalState {
        self.critical
    }
    pub fn active_records(&self) -> &[RecordId] {
        &self.active
    }
    pub fn queue(&self) -> &[Option<TextureKey>; 256] {
        &self.queue
    }
    pub fn counters(&self) -> QueueCounters {
        self.counters
    }
    pub fn pools(&self) -> &[Vec<PoolSlot>; 3] {
        &self.pools
    }
    pub fn record(&self, id: RecordId) -> Result<&ManagedRecord, ManagerError> {
        if id.arena != self.identity {
            return Err(ManagerError::ForeignWorld);
        }
        self.records
            .get(id.slot.wrapping_sub(1) as usize)
            .ok_or(ManagerError::UnknownRecord)
    }
    pub fn pool_slot(&self, id: PoolSlotId) -> Result<&PoolSlot, ManagerError> {
        if id.arena != self.identity {
            return Err(ManagerError::ForeignWorld);
        }
        self.pools
            .get(id.pool as usize)
            .and_then(|p| p.get(id.slot.wrapping_sub(1) as usize))
            .ok_or(ManagerError::UnknownPool)
    }
    pub(crate) fn record_count(&self, texture: TextureKey) -> usize {
        self.active
            .iter()
            .filter(|id| self.records[(id.slot - 1) as usize].owner == texture)
            .count()
    }
    pub(crate) fn has_links(&self, texture: TextureKey) -> bool {
        self.record_count(texture) > 0
            || self.queue.contains(&Some(texture))
            || self
                .pools
                .iter()
                .flatten()
                .any(|s| s.owner == Some(texture))
    }
    fn queue_linked(&self, texture: TextureKey) -> bool {
        self.queue.contains(&Some(texture))
    }
    pub(crate) fn validate_lifetime(
        &self,
        texture: TextureKey,
        flags: u32,
        record: Option<ManagedRecordKey>,
        graphics: Option<TextureGraphics>,
    ) -> Result<(), ManagerError> {
        if self.poisoned {
            return Err(ManagerError::Poisoned);
        }
        if !self.seeded || flags & 0x40 == 0 {
            return Err(ManagerError::Incoherent);
        }
        if let Some(record) = record {
            if !self
                .records
                .iter()
                .any(|r| r.owner == texture && r.allocation == record)
            {
                return Err(ManagerError::Incoherent);
            }
        }
        if self.queue_linked(texture)
            && (self.record_count(texture) == 0
                || flags & 0x40000000 == 0
                || self.counters.len <= 0)
        {
            return Err(ManagerError::Incoherent);
        }
        if self
            .pools
            .iter()
            .flatten()
            .any(|p| p.owner == Some(texture))
            && flags & 0x80000000 == 0
        {
            return Err(ManagerError::Incoherent);
        }
        if let Some(graphics) = graphics {
            if !self
                .pools
                .iter()
                .flatten()
                .any(|p| p.resources.graphics == Some(graphics))
            {
                return Err(ManagerError::Incoherent);
            }
        }
        Ok(())
    }
    pub(crate) fn validate_close(
        &self,
        texture: TextureKey,
        flags: u32,
        record: Option<ManagedRecordKey>,
        graphics: Option<TextureGraphics>,
    ) -> Result<(), ManagerError> {
        self.validate_lifetime(texture, flags, record, graphics)?;
        if self.record_count(texture) > 1 {
            return Err(ManagerError::Incoherent);
        }
        Ok(())
    }
    pub(crate) fn install(
        &mut self,
        input: ManagedWorldIntake,
    ) -> Result<Vec<ManagedBinding>, ManagerError> {
        if self.poisoned {
            return Err(ManagerError::Poisoned);
        }
        if self.seeded {
            return Err(ManagerError::AlreadySeeded);
        }
        if !(0..=256).contains(&input.counters.len) {
            return Err(ManagerError::InvalidCount);
        }
        let bound = |key: TextureKey| input.bindings.iter().find(|b| b.texture == key);
        for (i, b) in input.bindings.iter().enumerate() {
            if b.texture.arena() != self.identity
                || b.flags & 0x40 == 0
                || input.bindings[..i].iter().any(|p| p.texture == b.texture)
            {
                return Err(ManagerError::Incoherent);
            }
            if let Some(record) = b.record {
                if !input
                    .records
                    .iter()
                    .any(|r| r.owner == b.texture && r.allocation == record)
                {
                    return Err(ManagerError::Incoherent);
                }
            }
            if input.queue.contains(&Some(b.texture))
                && (input.counters.len <= 0
                    || b.flags & 0x40000000 == 0
                    || !input.records.iter().any(|r| r.owner == b.texture))
            {
                return Err(ManagerError::Incoherent);
            }
            if input
                .pools
                .iter()
                .flatten()
                .any(|p| p.owner == Some(b.texture))
                && b.flags & 0x80000000 == 0
            {
                return Err(ManagerError::Incoherent);
            }
            if let Some(g) = b.graphics {
                if b.flags & 0x80000000 == 0
                    || !input
                        .pools
                        .iter()
                        .flatten()
                        .any(|p| p.owner == Some(b.texture) && p.resources.graphics == Some(g))
                {
                    return Err(ManagerError::Incoherent);
                }
            }
        }
        for (i, r) in input.records.iter().enumerate() {
            if bound(r.owner).is_none()
                || r.allocation.0 == 0
                || r.file_name.is_some_and(|a| a.0 == 0)
            {
                return Err(ManagerError::InvalidIdentity);
            }
            let aliases = |a: u64| {
                input.records[..i]
                    .iter()
                    .any(|p| p.allocation.0 == a || p.file_name.is_some_and(|f| f.0 == a))
            };
            if aliases(r.allocation.0)
                || r.file_name
                    .is_some_and(|a| a.0 == r.allocation.0 || aliases(a.0))
            {
                return Err(ManagerError::InvalidIdentity);
            }
        }
        for key in input.queue.iter().flatten() {
            if bound(*key).is_none() {
                return Err(ManagerError::Incoherent);
            }
        }
        for p in input.pools.iter().flatten() {
            if p.owner.is_some_and(|k| bound(k).is_none())
                || p.resources.graphics.is_some_and(|g| g.0 == 0)
                || p.resources.surface.is_some_and(|s| s.0 == 0)
            {
                return Err(ManagerError::InvalidIdentity);
            }
        }
        u32::try_from(input.records.len()).map_err(|_| ManagerError::IdentityOverflow)?;
        for pool in &input.pools {
            u32::try_from(pool.len()).map_err(|_| ManagerError::IdentityOverflow)?;
        }
        self.records
            .try_reserve(input.records.len())
            .map_err(|_| ManagerError::HostAllocation)?;
        self.active
            .try_reserve(input.records.len())
            .map_err(|_| ManagerError::HostAllocation)?;
        for (i, p) in input.pools.iter().enumerate() {
            self.pools[i]
                .try_reserve(p.len())
                .map_err(|_| ManagerError::HostAllocation)?;
        }
        // Ownership transfers only after every validation and host reservation.
        for r in input.records {
            let id = RecordId {
                arena: self.identity,
                slot: self.records.len() as u32 + 1,
            };
            self.records.push(ManagedRecord {
                id,
                owner: r.owner,
                allocation: r.allocation,
                file_name: r.file_name,
                preserved: r.preserved,
                phase: RecordPhase::Live,
            });
            self.active.push(id);
        }
        for (i, pool) in input.pools.into_iter().enumerate() {
            for p in pool {
                let id = PoolSlotId {
                    arena: self.identity,
                    pool: i as u8,
                    slot: self.pools[i].len() as u32 + 1,
                };
                self.pools[i].push(PoolSlot {
                    id,
                    owner: p.owner,
                    preserved: p.preserved,
                    resources: p.resources,
                });
            }
        }
        self.queue = input.queue;
        self.counters = input.counters;
        self.seeded = true;
        Ok(input.bindings)
    }
    pub(crate) fn remove<E>(
        &mut self,
        texture: TextureKey,
        flags: &mut u32,
        effects: &mut dyn ManagedTextureEffects<Error = E>,
    ) -> Result<bool, ManagedOperationError<E>> {
        if self.poisoned {
            return Err(ManagerError::Poisoned.into());
        }
        if let Err(error) = effects.enter_texture_section() {
            self.poisoned = true;
            return Err(ManagedOperationError::Backend {
                stage: ManagedStage::Enter,
                error,
                leave_error: None,
            });
        }
        self.critical = CriticalState::Held;
        let body = (|| {
            if let Some(index) = self
                .active
                .iter()
                .rposition(|id| self.records[(id.slot - 1) as usize].owner == texture)
            {
                if self.counters.len > 0 && *flags & 0x40000000 != 0 {
                    for entry in self.queue.iter_mut().rev() {
                        if *entry == Some(texture) {
                            *entry = None;
                        }
                    }
                }
                let slot = (self.active[index].slot - 1) as usize;
                self.records[slot].phase = RecordPhase::FreeingFilename;
                effects
                    .delete_managed_filename(self.records[slot].file_name)
                    .map_err(|e| (ManagedStage::Filename, e))?;
                self.records[slot].phase = RecordPhase::FilenameFreed;
                self.records[slot].phase = RecordPhase::FreeingRecord;
                effects
                    .delete_managed_record(self.records[slot].allocation)
                    .map_err(|e| (ManagedStage::Record, e))?;
                self.records[slot].phase = RecordPhase::RecordFreed;
                self.active.swap_remove(index);
            }
            if *flags & 0x80000000 != 0 {
                for pool in &mut self.pools {
                    for entry in pool.iter_mut().rev() {
                        if entry.owner == Some(texture) {
                            entry.owner = None;
                        }
                    }
                }
            }
            *flags &= 0x3fffffff;
            Ok(())
        })();
        let leave = effects.leave_texture_section();
        self.critical = if leave.is_ok() {
            CriticalState::NotHeld
        } else {
            CriticalState::LeaveFailed
        };
        match (body, leave) {
            (Ok(()), Ok(())) => Ok(true),
            (Err((stage, error)), leave) => {
                self.poisoned = true;
                Err(ManagedOperationError::Backend {
                    stage,
                    error,
                    leave_error: leave.err(),
                })
            }
            (Ok(()), Err(error)) => {
                self.poisoned = true;
                Err(ManagedOperationError::Backend {
                    stage: ManagedStage::Leave,
                    error,
                    leave_error: None,
                })
            }
        }
    }
}
