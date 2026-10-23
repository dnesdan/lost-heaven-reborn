//! Bounded PC managed-texture creation/Request/removal; globals use descriptive names.
//! Original LS3DF.dll SHA-256:
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! RVA0x7ad20 AddTextureToTextureManager has original diagnostic-string name
//! provenance; 20-byte record, copied raw-byte filename, published texture
//! pointers, and explicit registry growth precede AL1. This excludes full Create.
//! RVA0x7aee0: reverse first-record match; conditional reverse256 queue clear;
//! filename delete, record delete, swap removal; three pools' reverse owner
//! clears; flags &=0x3fffffff; leave critical section; AL1. No worker,
//! full Create or graphics allocation is reconstructed here. ReleaseManagedTextures
//! is an upstream working name, RVA0x7a610: unload prefix, three per-pool
//! system/slot COM releases and allocator-backed array teardown; no Restore/worker.
//! RVA0x7a910 Request is an upstream working name: frame refresh and weak queue
//! production only; it does not load a texture.
//! Snapshot intake/typed identities and failure handling are host adapters.
use crate::itexture::{ManagedRecordKey, TextureGraphics, TextureKey, TextureSurface};
mod restore;
pub use restore::*;
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
    fn restore_effects(
        &mut self,
    ) -> Option<&mut dyn ManagedTextureRestoreEffects<Error = Self::Error>> {
        None
    }
    /// Optional unavailable capability, never a successful substitute. Stable
    /// capability/identity domain is required across serialized callbacks.
    fn creation_effects(
        &mut self,
    ) -> Option<&mut dyn ManagedTextureCreationEffects<Error = Self::Error>> {
        None
    }
    /// Storage capability for Request's record+0x10 write; absent rejects
    /// before entering the critical section.
    fn request_effects(
        &mut self,
    ) -> Option<&mut dyn ManagedTextureRequestEffects<Error = Self::Error>> {
        None
    }
    fn release_effects(
        &mut self,
    ) -> Option<&mut dyn ManagedTextureReleaseEffects<Error = Self::Error>> {
        None
    }
    fn enter_texture_section(&mut self) -> Result<(), Self::Error>;
    fn leave_texture_section(&mut self) -> Result<(), Self::Error>;
    /// The original delete is unconditional, including the null pointer case.
    fn delete_managed_filename(
        &mut self,
        allocation: Option<ManagedFilenameAllocation>,
    ) -> Result<(), Self::Error>;
    fn delete_managed_record(&mut self, allocation: ManagedRecordKey) -> Result<(), Self::Error>;
}
/// ReleaseManagedTextures is an upstream working name, PC RVA0x7a610.
/// Pure, stable capability checks cover every call before the unload prefix.
/// Callbacks cannot reenter or mutate admitted storage; owned references remain
/// valid until their scheduled release, including repeated aliases. Aliased
/// COM identities each represent a separate owned reference, never deduplicated.
pub trait ManagedTextureReleaseEffects {
    type Error;
    fn can_release_surface(&self, surface: TextureSurface) -> bool;
    fn can_release_texture_graphics(&self, texture: TextureGraphics) -> bool;
    fn can_delete_managed_pool(&self, allocation: Option<ManagedPoolAllocation>) -> bool;
    fn release_surface(&mut self, surface: TextureSurface) -> Result<(), Self::Error>;
    fn release_texture_graphics(&mut self, texture: TextureGraphics) -> Result<(), Self::Error>;
    fn delete_managed_pool(
        &mut self,
        allocation: Option<ManagedPoolAllocation>,
    ) -> Result<(), Self::Error>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ManagedPoolAllocation(pub u64);
/// Native list data+0, capacity+4, grow+12; count+8 is the pool slot length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManagedPoolStorage {
    pub allocation: Option<ManagedPoolAllocation>,
    pub capacity: u32,
    pub growth: u32,
}
/// Separate owned references and allocator-backed storage, adopted atomically.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManagedReleaseIntake {
    pub system: [PoolResources; 3],
    pub pools: [ManagedPoolStorage; 3],
    pub pool_memory: [u32; 3],
}
/// Explicit storage write for Request (upstream working name), RVA0x7a910.
/// Serialized callbacks must preserve capability identity and cannot reenter.
pub trait ManagedTextureRequestEffects {
    type Error;
    /// Pure preflight: record belongs to this backend and has writable storage
    /// for the dword at +0x10 of its native 20-byte record.
    fn can_write_managed_last_frame(&self, record: ManagedRecordKey) -> bool;
    fn write_managed_last_frame(
        &mut self,
        record: ManagedRecordKey,
        frame: u32,
    ) -> Result<(), Self::Error>;
}
/// Required storage/allocator effects for original AddTextureToTextureManager,
/// RVA0x7ad20. Record/array allocation None is an unsupported native unchecked
/// path, while filename None is the original false result. No callback reentry
/// or mutation of format globals/count is admitted. Allocator identities must
/// remain distinct from every historical allocation in this world.
pub trait ManagedTextureCreationEffects {
    type Error;
    fn allocate_managed_record(
        &mut self,
        bytes: u32,
    ) -> Result<Option<ManagedRecordKey>, Self::Error>;
    fn write_managed_record(
        &mut self,
        record: ManagedRecordKey,
        value: RecordWrite,
    ) -> Result<(), Self::Error>;
    fn allocate_managed_filename(
        &mut self,
        bytes: u32,
    ) -> Result<Option<ManagedFilenameAllocation>, Self::Error>;
    fn copy_managed_filename(
        &mut self,
        allocation: ManagedFilenameAllocation,
        bytes: &[u8],
    ) -> Result<(), Self::Error>;
    fn allocate_managed_registry(
        &mut self,
        bytes: u32,
    ) -> Result<Option<ManagedRegistryAllocation>, Self::Error>;
    fn write_managed_registry_entry(
        &mut self,
        array: ManagedRegistryAllocation,
        index: u32,
        record: ManagedRecordKey,
    ) -> Result<(), Self::Error>;
    /// Includes the original null old-array delete call.
    fn delete_managed_registry(
        &mut self,
        array: Option<ManagedRegistryAllocation>,
    ) -> Result<(), Self::Error>;
    fn managed_texture_low_memory(&mut self) -> Result<(), Self::Error>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ManagedRegistryAllocation(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordWrite {
    Owner(TextureKey),
    LastFrame(u32),
    Format(u32),
    Pool(i32),
    Filename(Option<ManagedFilenameAllocation>),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreationFormats {
    pub dxt: u32,
    pub normal: u32,
}
/// Explicit host intake, not reconstructed manager constructor defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegistryIntake {
    pub capacity: u32,
    pub growth: u32,
    pub allocation: Option<ManagedRegistryAllocation>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManagedRecordFields {
    pub format: u32,
    pub pool: i32,
    pub last_frame: u32,
}
/// Interrupted host diagnostics. `fields` and `filename_bytes` are planned
/// values computed before effects, not evidence of initialized native storage.
/// `stage` identifies the current/last attempted effect; `published` and `copied`
/// record completed host publication/copy steps. A callback may perform its
/// effect before returning Err, so its completion remains ambiguous.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreationProgress {
    pub record: Option<ManagedRecordKey>,
    pub filename: Option<ManagedFilenameAllocation>,
    pub filename_bytes: Vec<u8>,
    pub fields: ManagedRecordFields,
    pub stage: ManagedStage,
    pub published: bool,
    pub new_array: Option<ManagedRegistryAllocation>,
    pub copied: u32,
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
    AllocateRecord,
    WriteRecord,
    AllocateFilename,
    CopyFilename,
    Diagnostic,
    AllocateRegistry,
    CopyRegistry,
    DeleteRegistry,
    AppendRegistry,
    ReleaseSurface,
    ReleaseGraphics,
    DeletePool,
    CreateTexture,
    GetSurfaceLevel,
    GetSurfaceDesc,
    AllocatePool,
    CopyPool,
    WritePoolOwner,
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
    CreationUnavailable,
    RequestUnavailable,
    ReleaseUnavailable,
    UnsupportedAllocation,
    RestoreUnavailable,
    UnsupportedGraphics,
    InvalidFilename,
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
    /// Release7a610 accesses texture+4 and surface+8 in each0x2c slot;
    /// removal7aee0 itself accesses only owner+0 and the0x2c stride.
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
    /// Opaque intake metadata, not a byte-addressed native record image.
    pub preserved: Vec<u8>,
    pub phase: RecordPhase,
    pub fields: Option<ManagedRecordFields>,
    /// Last acknowledged record+0x10 storage write; seeded storage begins unknown.
    /// A failed callback may have written storage without acknowledging it.
    pub last_frame: Option<u32>,
    pub filename_bytes: Option<Vec<u8>>,
}
#[derive(Debug, PartialEq, Eq)]
pub struct PoolSlot {
    pub id: PoolSlotId,
    pub owner: Option<TextureKey>,
    pub preserved: [u8; 40],
    pub resources: PoolResources,
}
/// Private mutation, immutable diagnostics. Pool-owned resources survive owner
/// clears; explicit configured release is required, and Drop cannot free them.
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
    registry: Option<RegistryIntake>,
    creation: Option<CreationProgress>,
    allocation_history: Vec<u64>,
    registry_swap: Option<(u32, RecordId)>,
    release: Option<ManagedReleaseIntake>,
    released_borrows: Vec<TextureGraphics>,
    restore: Option<RestoreProgress>,
    next_pool_slot: [u32; 3],
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
            registry: None,
            creation: None,
            allocation_history: Vec::new(),
            registry_swap: None,
            release: None,
            released_borrows: Vec::new(),
            restore: None,
            next_pool_slot: [1; 3],
        }
    }
    /// Interrupted swap-copy after the original count decrement.
    pub fn pending_registry_swap(&self) -> Option<(u32, RecordId)> {
        self.registry_swap
    }
    pub fn registry(&self) -> Option<RegistryIntake> {
        self.registry
    }
    pub fn creation_progress(&self) -> Option<&CreationProgress> {
        self.creation.as_ref()
    }
    pub(crate) fn configure_registry(
        &mut self,
        input: RegistryIntake,
        forbidden: &[u64],
    ) -> Result<(), ManagerError> {
        if self.poisoned {
            return Err(ManagerError::Poisoned);
        }
        if self.registry.is_some() {
            return Err(ManagerError::AlreadySeeded);
        }
        if input.capacity > i32::MAX as u32
            || input.growth == 0
            || input.growth > i32::MAX as u32
            || input.capacity < self.active.len() as u32
            || input.capacity.checked_mul(4).is_none()
            || (input.capacity == 0) != input.allocation.is_none()
        {
            return Err(ManagerError::InvalidCount);
        }
        if let Some(a) = input.allocation {
            if self.aliases(a.0, forbidden) {
                return Err(ManagerError::InvalidIdentity);
            }
            self.allocation_history
                .try_reserve(1)
                .map_err(|_| ManagerError::HostAllocation)?;
            self.allocation_history.push(a.0);
        }
        self.registry = Some(input);
        self.seeded = true;
        Ok(())
    }
    fn aliases(&self, raw: u64, forbidden: &[u64]) -> bool {
        raw == 0
            || forbidden.contains(&raw)
            || self.allocation_history.contains(&raw)
            || self
                .records
                .iter()
                .any(|r| r.allocation.0 == raw || r.file_name.is_some_and(|a| a.0 == raw))
    }
    pub(crate) fn preflight_add(&mut self, name: &[u8]) -> Result<(), ManagerError> {
        if self.poisoned {
            return Err(ManagerError::Poisoned);
        }
        if !(5..=253).contains(&name.len()) || name.contains(&0) {
            return Err(ManagerError::InvalidFilename);
        }
        let registry = self.registry.ok_or(ManagerError::CreationUnavailable)?;
        let count = u32::try_from(self.active.len()).map_err(|_| ManagerError::InvalidCount)?;
        if count >= registry.capacity {
            registry
                .capacity
                .checked_add(registry.growth)
                .filter(|&v| v <= i32::MAX as u32)
                .and_then(|v| v.checked_mul(4))
                .ok_or(ManagerError::InvalidCount)?;
        }
        u32::try_from(self.records.len() + 1).map_err(|_| ManagerError::IdentityOverflow)?;
        self.records
            .try_reserve(1)
            .map_err(|_| ManagerError::HostAllocation)?;
        self.active
            .try_reserve(1)
            .map_err(|_| ManagerError::HostAllocation)?;
        self.allocation_history
            .try_reserve(3)
            .map_err(|_| ManagerError::HostAllocation)?;
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn add<E>(
        &mut self,
        texture: TextureKey,
        name: &[u8],
        flags: u32,
        formats: CreationFormats,
        forbidden: &[u64],
        effects: &mut dyn ManagedTextureEffects<Error = E>,
        mut publish: impl FnMut(ManagedRecordKey),
    ) -> Result<bool, ManagedOperationError<E>> {
        self.preflight_add(name)?;
        if effects.creation_effects().is_none() {
            return Err(ManagerError::CreationUnavailable.into());
        }
        let stem = name.len() - 4;
        let format = if flags & 0x200 != 0 && formats.dxt != 0 {
            formats.dxt
        } else {
            formats.normal
        };
        let mut bytes = Vec::new();
        bytes
            .try_reserve(stem + 7)
            .map_err(|_| ManagerError::HostAllocation)?;
        bytes.extend_from_slice(&name[..stem]);
        bytes.extend_from_slice(if flags & 0x200 != 0 && formats.dxt != 0 {
            b"_1.DX1\0"
        } else if formats.normal == 0x17 {
            b"_1.565\0"
        } else {
            b"_5.565\0"
        });
        self.creation = Some(CreationProgress {
            record: None,
            filename: None,
            filename_bytes: bytes,
            fields: ManagedRecordFields {
                format,
                pool: name[stem - 1] as i8 as i32 - 0x30,
                last_frame: 0,
            },
            stage: ManagedStage::AllocateRecord,
            published: false,
            new_array: None,
            copied: 0,
        });
        macro_rules! effect {
            ($stage:ident, $call:expr) => {{
                self.creation.as_mut().unwrap().stage = ManagedStage::$stage;
                $call.map_err(|error| ManagedOperationError::Backend {
                    stage: ManagedStage::$stage,
                    error,
                    leave_error: None,
                })?
            }};
        }
        let result = (|| {
            let record = effect!(
                AllocateRecord,
                effects
                    .creation_effects()
                    .unwrap()
                    .allocate_managed_record(20)
            )
            .ok_or(ManagerError::UnsupportedAllocation)?;
            self.creation.as_mut().unwrap().record = Some(record);
            if self.aliases(record.0, forbidden) {
                return Err(ManagerError::InvalidIdentity.into());
            }
            self.allocation_history.push(record.0);
            effect!(
                WriteRecord,
                effects
                    .creation_effects()
                    .unwrap()
                    .write_managed_record(record, RecordWrite::Owner(texture))
            );
            effect!(
                WriteRecord,
                effects
                    .creation_effects()
                    .unwrap()
                    .write_managed_record(record, RecordWrite::LastFrame(0))
            );
            let pool = self.creation.as_ref().unwrap().fields.pool;
            if flags & 0x200 != 0 && formats.dxt != 0 {
                effect!(
                    WriteRecord,
                    effects
                        .creation_effects()
                        .unwrap()
                        .write_managed_record(record, RecordWrite::Format(format))
                );
                effect!(
                    WriteRecord,
                    effects
                        .creation_effects()
                        .unwrap()
                        .write_managed_record(record, RecordWrite::Pool(pool))
                );
            } else {
                effect!(
                    WriteRecord,
                    effects
                        .creation_effects()
                        .unwrap()
                        .write_managed_record(record, RecordWrite::Pool(pool))
                );
                effect!(
                    WriteRecord,
                    effects
                        .creation_effects()
                        .unwrap()
                        .write_managed_record(record, RecordWrite::Format(format))
                );
            }
            let size = self.creation.as_ref().unwrap().filename_bytes.len() as u32;
            let file = effect!(
                AllocateFilename,
                effects
                    .creation_effects()
                    .unwrap()
                    .allocate_managed_filename(size)
            );
            self.creation.as_mut().unwrap().filename = file;
            effect!(
                WriteRecord,
                effects
                    .creation_effects()
                    .unwrap()
                    .write_managed_record(record, RecordWrite::Filename(file))
            );
            let Some(file) = file else {
                effect!(Record, effects.delete_managed_record(record));
                effect!(
                    Diagnostic,
                    effects
                        .creation_effects()
                        .unwrap()
                        .managed_texture_low_memory()
                );
                return Ok(false);
            };
            if self.aliases(file.0, forbidden) {
                return Err(ManagerError::InvalidIdentity.into());
            }
            self.allocation_history.push(file.0);
            effect!(
                CopyFilename,
                effects
                    .creation_effects()
                    .unwrap()
                    .copy_managed_filename(file, &self.creation.as_ref().unwrap().filename_bytes)
            );
            publish(record);
            self.creation.as_mut().unwrap().published = true;
            let count = self.active.len() as u32;
            let registry = self.registry.as_mut().unwrap();
            if count >= registry.capacity {
                registry.capacity += registry.growth;
                let capacity = registry.capacity;
                let array = effect!(
                    AllocateRegistry,
                    effects
                        .creation_effects()
                        .unwrap()
                        .allocate_managed_registry(capacity * 4)
                )
                .ok_or(ManagerError::UnsupportedAllocation)?;
                self.creation.as_mut().unwrap().new_array = Some(array);
                if self.aliases(array.0, forbidden) {
                    return Err(ManagerError::InvalidIdentity.into());
                }
                self.allocation_history.push(array.0);
                for (i, id) in self.active.iter().enumerate() {
                    let old = self.records[(id.slot - 1) as usize].allocation;
                    effect!(
                        CopyRegistry,
                        effects
                            .creation_effects()
                            .unwrap()
                            .write_managed_registry_entry(array, i as u32, old)
                    );
                    self.creation.as_mut().unwrap().copied += 1;
                }
                effect!(
                    DeleteRegistry,
                    effects
                        .creation_effects()
                        .unwrap()
                        .delete_managed_registry(self.registry.unwrap().allocation)
                );
                self.registry.as_mut().unwrap().allocation = Some(array);
            }
            let array = self.registry.unwrap().allocation.unwrap();
            effect!(
                AppendRegistry,
                effects
                    .creation_effects()
                    .unwrap()
                    .write_managed_registry_entry(array, count, record)
            );
            let progress = self.creation.take().unwrap();
            let id = RecordId {
                arena: self.identity,
                slot: self.records.len() as u32 + 1,
            };
            self.records.push(ManagedRecord {
                id,
                owner: texture,
                allocation: record,
                file_name: Some(file),
                preserved: Vec::new(),
                phase: RecordPhase::Live,
                last_frame: Some(progress.fields.last_frame),
                fields: Some(progress.fields),
                filename_bytes: Some(progress.filename_bytes),
            });
            self.active.push(id);
            Ok(true)
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
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
            .and_then(|p| p.iter().find(|slot| slot.id == id))
            .ok_or(ManagerError::UnknownPool)
    }
    pub(crate) fn requires_creation_effects(&self) -> bool {
        self.registry.is_some()
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
                && !self.released_borrows.contains(&graphics)
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
        allow_unloaded_owners: bool,
    ) -> Result<Vec<ManagedBinding>, ManagerError> {
        if self.poisoned {
            return Err(ManagerError::Poisoned);
        }
        if self.seeded {
            return Err(ManagerError::AlreadySeeded);
        }
        // Host intake policy excludes negative signed lengths; Request itself
        // retains the native signed comparison and never counts occupied slots.
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
                && !allow_unloaded_owners
            {
                return Err(ManagerError::Incoherent);
            }
            if let Some(g) = b.graphics {
                if (!allow_unloaded_owners && b.flags & 0x80000000 == 0)
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
            // Native nonzero counts include unsafe negative representations.
            // Safe intake admits only nonnegative signed list extents.
            i32::try_from(pool.len()).map_err(|_| ManagerError::InvalidCount)?;
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
                fields: None,
                last_frame: None,
                filename_bytes: None,
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
        self.next_pool_slot = std::array::from_fn(|i| self.pools[i].len() as u32 + 1);
        self.counters = input.counters;
        self.seeded = true;
        Ok(input.bindings)
    }
    pub fn release_state(&self) -> Option<&ManagedReleaseIntake> {
        self.release.as_ref()
    }
    pub(crate) fn configure_release(
        &mut self,
        input: ManagedReleaseIntake,
        forbidden: &[u64],
    ) -> Result<(), ManagerError> {
        if self.poisoned {
            return Err(ManagerError::Poisoned);
        }
        if self.release.is_some() {
            return Err(ManagerError::AlreadySeeded);
        }
        if !self.seeded {
            return Err(ManagerError::Incoherent);
        }
        for (i, storage) in input.pools.iter().enumerate() {
            if storage.capacity > i32::MAX as u32
                || storage.capacity < self.pools[i].len() as u32
                || storage.capacity.checked_mul(0x2c).is_none()
                || (storage.capacity == 0) != storage.allocation.is_none()
            {
                return Err(ManagerError::InvalidCount);
            }
            if let Some(a) = storage.allocation {
                if self.aliases(a.0, forbidden)
                    || input.pools[..i].iter().any(|p| p.allocation == Some(a))
                {
                    return Err(ManagerError::InvalidIdentity);
                }
            }
        }
        if input
            .system
            .iter()
            .any(|p| p.graphics.is_some_and(|g| g.0 == 0) || p.surface.is_some_and(|s| s.0 == 0))
        {
            return Err(ManagerError::InvalidIdentity);
        }
        self.allocation_history
            .try_reserve(3)
            .map_err(|_| ManagerError::HostAllocation)?;
        self.released_borrows
            .try_reserve(self.pools.iter().map(Vec::len).sum())
            .map_err(|_| ManagerError::HostAllocation)?;
        self.allocation_history
            .extend(input.pools.iter().filter_map(|p| p.allocation.map(|a| a.0)));
        self.release = Some(input);
        Ok(())
    }
    /// Inlines the native unload prefix, then releases resources outside lock.
    /// Failed callbacks retain the attempted pointer and poison without retries.
    pub(crate) fn release<E>(
        &mut self,
        clear_loaded: impl FnMut(TextureKey),
        effects: &mut dyn ManagedTextureEffects<Error = E>,
    ) -> Result<(), ManagedOperationError<E>> {
        if self.poisoned {
            return Err(ManagerError::Poisoned.into());
        }
        let state = self.release.ok_or(ManagerError::ReleaseUnavailable)?;
        let capability = effects
            .release_effects()
            .ok_or(ManagerError::ReleaseUnavailable)?;
        let valid = state.system.iter().all(|p| {
            p.surface.is_none_or(|s| capability.can_release_surface(s))
                && p.graphics
                    .is_none_or(|g| capability.can_release_texture_graphics(g))
        }) && state
            .pools
            .iter()
            .all(|p| capability.can_delete_managed_pool(p.allocation))
            && self.pools.iter().flatten().all(|p| {
                p.resources
                    .surface
                    .is_some_and(|s| capability.can_release_surface(s))
                    && p.resources
                        .graphics
                        .is_some_and(|g| capability.can_release_texture_graphics(g))
            });
        if !valid {
            return Err(ManagerError::ReleaseUnavailable.into());
        }
        self.unload(clear_loaded, effects)?;
        let result = (|| {
            let state = self.release.as_mut().unwrap();
            for (i, pool) in self.pools.iter_mut().enumerate() {
                if let Some(s) = state.system[i].surface {
                    effects
                        .release_effects()
                        .unwrap()
                        .release_surface(s)
                        .map_err(|e| (ManagedStage::ReleaseSurface, e))?;
                    state.system[i].surface = None;
                }
                if let Some(g) = state.system[i].graphics {
                    effects
                        .release_effects()
                        .unwrap()
                        .release_texture_graphics(g)
                        .map_err(|e| (ManagedStage::ReleaseGraphics, e))?;
                    state.system[i].graphics = None;
                }
                for slot in pool.iter_mut().rev() {
                    effects
                        .release_effects()
                        .unwrap()
                        .release_surface(slot.resources.surface.unwrap())
                        .map_err(|e| (ManagedStage::ReleaseSurface, e))?;
                    // Native slot pointers are not nulled before the array free.
                    effects
                        .release_effects()
                        .unwrap()
                        .release_texture_graphics(slot.resources.graphics.unwrap())
                        .map_err(|e| (ManagedStage::ReleaseGraphics, e))?;
                }
                effects
                    .release_effects()
                    .unwrap()
                    .delete_managed_pool(state.pools[i].allocation)
                    .map_err(|e| (ManagedStage::DeletePool, e))?;
                self.released_borrows
                    .extend(pool.iter().filter_map(|s| s.resources.graphics));
                state.pools[i].allocation = None;
                pool.clear();
                state.pools[i].capacity = 0;
                state.pool_memory[i] = 0;
            }
            Ok(())
        })();
        result.map_err(|(stage, error)| {
            self.poisoned = true;
            ManagedOperationError::Backend {
                stage,
                error,
                leave_error: None,
            }
        })
    }
    /// Standalone unload (upstream working name), PC RVA0x7afe0.
    /// Caller preflights every weak owner as live before supplying flag access.
    pub(crate) fn unload<E>(
        &mut self,
        mut clear_loaded: impl FnMut(TextureKey),
        effects: &mut dyn ManagedTextureEffects<Error = E>,
    ) -> Result<(), ManagedOperationError<E>> {
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
        for pool in &mut self.pools {
            for slot in pool.iter_mut().rev() {
                if let Some(owner) = slot.owner {
                    clear_loaded(owner);
                    slot.owner = None;
                }
            }
        }
        match effects.leave_texture_section() {
            Ok(()) => {
                self.critical = CriticalState::NotHeld;
                Ok(())
            }
            Err(error) => {
                self.critical = CriticalState::LeaveFailed;
                self.poisoned = true;
                Err(ManagedOperationError::Backend {
                    stage: ManagedStage::Leave,
                    error,
                    leave_error: None,
                })
            }
        }
    }
    /// Request is a descriptive upstream working name, not a recovered PS2
    /// symbol. Caller has tested managed bit0x40. No worker/loading runs here.
    pub(crate) fn request<E>(
        &mut self,
        texture: TextureKey,
        record: Option<ManagedRecordKey>,
        flags: &mut u32,
        frame: u32,
        effects: &mut dyn ManagedTextureEffects<Error = E>,
    ) -> Result<bool, ManagedOperationError<E>> {
        if self.poisoned {
            return Err(ManagerError::Poisoned.into());
        }
        let slot = self
            .records
            .iter()
            .position(|r| {
                r.owner == texture
                    && Some(r.allocation) == record
                    && r.phase == RecordPhase::Live
                    && self.active.contains(&r.id)
            })
            .ok_or(ManagerError::UnknownRecord)?;
        // All host guards precede the first external effect. A stale pointer
        // after direct removal is never dereferenced, including loaded requests.
        if *flags & 0xc0000000 == 0 && self.counters.len < 256 && self.counters.put >= 256 {
            return Err(ManagerError::InvalidCount.into());
        }
        if !effects.request_effects().is_some_and(|storage| {
            storage.can_write_managed_last_frame(self.records[slot].allocation)
        }) {
            return Err(ManagerError::RequestUnavailable.into());
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
            effects
                .request_effects()
                .unwrap()
                .write_managed_last_frame(self.records[slot].allocation, frame)
                .map_err(|e| (ManagedStage::WriteRecord, e))?;
            self.records[slot].last_frame = Some(frame);
            if let Some(fields) = &mut self.records[slot].fields {
                fields.last_frame = frame;
            }
            if *flags & 0x80000000 != 0 {
                return Ok(true);
            }
            if *flags & 0x40000000 == 0 && self.counters.len < 256 {
                // Weak owner overwrite: there is no vacancy/occupancy test.
                self.queue[self.counters.put as usize] = Some(texture);
                self.counters.put += 1;
                if self.counters.put as i32 >= 256 {
                    self.counters.put = 0;
                }
                self.counters.len += 1;
                *flags |= 0x40000000;
            }
            Ok(false)
        })();
        let leave = effects.leave_texture_section();
        self.critical = if leave.is_ok() {
            CriticalState::NotHeld
        } else {
            CriticalState::LeaveFailed
        };
        match (body, leave) {
            (Ok(value), Ok(())) => Ok(value),
            (Err((stage, error)), leave) => {
                self.poisoned = true;
                Err(ManagedOperationError::Backend {
                    stage,
                    error,
                    leave_error: leave.err(),
                })
            }
            (Ok(_), Err(error)) => {
                self.poisoned = true;
                Err(ManagedOperationError::Backend {
                    stage: ManagedStage::Leave,
                    error,
                    leave_error: None,
                })
            }
        }
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
        if self.registry.is_some() && effects.creation_effects().is_none() {
            return Err(ManagerError::CreationUnavailable.into());
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
                // Native count publication precedes the array copy. Keep the
                // former last identity available if the external write fails.
                let last_id = self.active.pop().unwrap();
                let remaining = self.active.len();
                if remaining != 0 {
                    self.registry_swap = Some((index as u32, last_id));
                    if let Some(registry) = self.registry {
                        let last = self.records[(last_id.slot - 1) as usize].allocation;
                        effects
                            .creation_effects()
                            .unwrap()
                            .write_managed_registry_entry(
                                registry.allocation.unwrap(),
                                index as u32,
                                last,
                            )
                            .map_err(|e| (ManagedStage::CopyRegistry, e))?;
                    }
                    if index < remaining {
                        self.active[index] = last_id;
                    }
                    self.registry_swap = None;
                }
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
