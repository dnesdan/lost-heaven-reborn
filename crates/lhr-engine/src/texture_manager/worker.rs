//! RenderThreadProc is an upstream working name, PC RVA 0x7a9b0.
//! Finite serialized host schedule of the complete outer loop; no GPU claim.
use super::*;
use crate::itexture::{ITexture, Observed};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkerInput {
    pub max_running_checks: u32,
    pub max_slots: u32,
    /// Original dimension table, independent of surface descriptor output.
    pub dimensions: [u32; 3],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManagedLockedRect {
    pub mapping: u64,
    pub pitch: i32,
    pub writable_bytes: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManagedFile(pub u64);
/// Interrupted host prefix; effects returning Err may already have completed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkerProgress {
    pub stage: ManagedStage,
    pub temporary_reference: Option<TextureKey>,
    pub pending_pool: Option<ManagedPoolAllocation>,
    pub copied: u32,
    pub mapping: Option<ManagedLockedRect>,
    pub file: Option<ManagedFile>,
    pub complete: bool,
}
/// Required effects in the same identity domain as Restore and Request.
/// Callbacks are serialized, cannot reenter or mutate manager/texture storage.
/// HRESULT and DTA result codes are ignored natively; Err means interruption,
/// possibly after an effect. No rollback, cleanup retry or invented recovery.
/// Lock maps the actual surface; read writes directly to that mapping, ignoring
/// pitch. Valid nonnull outputs and sufficient contiguous extent are required.
pub trait ManagedTextureWorkerEffects {
    type Error;
    fn managed_worker_running(&mut self) -> Result<bool, Self::Error>;
    fn managed_worker_frame(&mut self) -> Result<u32, Self::Error>;
    fn managed_worker_sleep(&mut self, milliseconds: u32) -> Result<(), Self::Error>;
    fn lock_managed_surface(
        &mut self,
        surface: TextureSurface,
        flags: u32,
    ) -> Result<Option<ManagedLockedRect>, Self::Error>;
    fn open_managed_file(
        &mut self,
        filename: ManagedFilenameAllocation,
        mode: u32,
    ) -> Result<Option<ManagedFile>, Self::Error>;
    fn seek_managed_file(
        &mut self,
        file: ManagedFile,
        offset: i32,
        origin: u32,
    ) -> Result<(), Self::Error>;
    fn read_managed_file(
        &mut self,
        file: ManagedFile,
        mapping: ManagedLockedRect,
        bytes: u32,
    ) -> Result<(), Self::Error>;
    fn close_managed_file(&mut self, file: ManagedFile) -> Result<(), Self::Error>;
    fn unlock_managed_surface(&mut self, surface: TextureSurface) -> Result<(), Self::Error>;
    /// Invokes the admitted nondeleting ITexture Release target. Storage is
    /// projected below only after acknowledgement; Err may already decrement.
    fn release_managed_worker_reference(
        &mut self,
        texture: TextureKey,
        references: u32,
        graphics: TextureGraphics,
        flags: u32,
    ) -> Result<(), Self::Error>;
    fn update_managed_texture(
        &mut self,
        source: TextureGraphics,
        destination: TextureGraphics,
    ) -> Result<(), Self::Error>;
}
impl TextureManager {
    pub fn worker_progress(&self) -> Option<&WorkerProgress> {
        self.worker.as_ref()
    }

    /// Host metadata admission for seeded native record storage. This does not
    /// write native storage; the caller attests fields reflect the same backend.
    pub(crate) fn configure_worker_records(
        &mut self,
        input: &[(RecordId, ManagedRecordFields)],
    ) -> Result<(), ManagerError> {
        if self.poisoned {
            return Err(ManagerError::Poisoned);
        }
        for (n, (id, _)) in input.iter().enumerate() {
            let r = self.record(*id)?;
            if r.fields.is_some()
                || r.phase != RecordPhase::Live
                || !self.active.contains(id)
                || input[..n].iter().any(|(prior, _)| prior == id)
            {
                return Err(ManagerError::Incoherent);
            }
        }
        for (id, fields) in input {
            let r = &mut self.records[(id.slot - 1) as usize];
            r.fields = Some(*fields);
            r.last_frame = Some(fields.last_frame);
        }
        Ok(())
    }
    fn worker_enter<E>(
        &mut self,
        effects: &mut dyn ManagedTextureEffects<Error = E>,
    ) -> Result<(), ManagedOperationError<E>> {
        self.worker.as_mut().unwrap().stage = ManagedStage::Enter;
        effects
            .enter_texture_section()
            .map_err(|error| ManagedOperationError::Backend {
                stage: ManagedStage::Enter,
                error,
                leave_error: None,
            })?;
        self.critical = CriticalState::Held;
        Ok(())
    }
    fn worker_leave<E>(
        &mut self,
        effects: &mut dyn ManagedTextureEffects<Error = E>,
    ) -> Result<(), ManagedOperationError<E>> {
        self.worker.as_mut().unwrap().stage = ManagedStage::Leave;
        match effects.leave_texture_section() {
            Ok(()) => {
                self.critical = CriticalState::NotHeld;
                Ok(())
            }
            Err(error) => {
                self.critical = CriticalState::LeaveFailed;
                Err(ManagedOperationError::Backend {
                    stage: ManagedStage::Leave,
                    error,
                    leave_error: None,
                })
            }
        }
    }
    fn worker_advance(&mut self) {
        self.consumed_queue[self.counters.out as usize] = true;
        self.counters.out = self.counters.out.wrapping_add(1);
        if self.counters.out as i32 >= 256 {
            self.counters.out = 0;
        }
        self.counters.len -= 1;
    }
    /// Requires all weak owners live, coherent admitted records, and nonzero
    /// original references. This excludes native null/negative/overflow paths.
    pub(crate) fn worker<E>(
        &mut self,
        input: WorkerInput,
        forbidden: &[u64],
        textures: &mut [Option<ITexture>],
        effects: &mut dyn ManagedTextureEffects<Error = E>,
    ) -> Result<u32, ManagedOperationError<E>> {
        if self.poisoned {
            return Err(ManagerError::Poisoned.into());
        }
        if effects.worker_effects().is_none() {
            return Err(ManagerError::WorkerUnavailable.into());
        }
        self.worker = Some(WorkerProgress {
            stage: ManagedStage::Running,
            temporary_reference: None,
            pending_pool: None,
            copied: 0,
            mapping: None,
            file: None,
            complete: false,
        });
        macro_rules! effect {
            ($stage:ident,$call:expr) => {{
                self.worker.as_mut().unwrap().stage = ManagedStage::$stage;
                $call.map_err(|error| ManagedOperationError::Backend {
                    stage: ManagedStage::$stage,
                    error,
                    leave_error: None,
                })?
            }};
        }
        let result = (|| {
            for _ in 0..input.max_running_checks {
                if !effect!(
                    Running,
                    effects.worker_effects().unwrap().managed_worker_running()
                ) {
                    self.worker.as_mut().unwrap().complete = true;
                    return Ok(0);
                }
                if self.counters.len < 0 || self.counters.len > 256 || self.counters.out >= 256 {
                    return Err(ManagerError::InvalidCount.into());
                }
                if self.counters.len == 0 {
                    effect!(
                        Sleep,
                        effects.worker_effects().unwrap().managed_worker_sleep(0)
                    );
                    continue;
                }
                self.worker_enter(effects)?;
                let selected = loop {
                    if self.counters.len == 0 {
                        break None;
                    }
                    if let Some(key) = self.queue[self.counters.out as usize] {
                        let record = self
                            .records
                            .iter()
                            .position(|r| {
                                r.owner == key
                                    && Some(r.allocation)
                                        == textures[(key.slot() - 1) as usize]
                                            .as_ref()
                                            .unwrap()
                                            .managed_record
                                    && self.active.contains(&r.id)
                                    && r.phase == RecordPhase::Live
                            })
                            .ok_or(ManagerError::UnknownRecord)?;
                        let frame = effect!(
                            Frame,
                            effects.worker_effects().unwrap().managed_worker_frame()
                        );
                        let age = self.records[record]
                            .last_frame
                            .ok_or(ManagerError::UnsupportedWorker)?;
                        if age.wrapping_add(1) >= frame {
                            break Some((key, record));
                        }
                        textures[(key.slot() - 1) as usize].as_mut().unwrap().flags &= !0x40000000;
                    }
                    self.worker_advance();
                };
                let Some((key, record)) = selected else {
                    self.worker_leave(effects)?;
                    continue;
                };
                let texture = textures[(key.slot() - 1) as usize].as_mut().unwrap();
                if texture.references == 0 || texture.references == u32::MAX {
                    return Err(ManagerError::UnsupportedWorker.into());
                }
                texture.references += 1;
                self.worker.as_mut().unwrap().temporary_reference = Some(key);
                self.worker_leave(effects)?;
                let fields = self.records[record]
                    .fields
                    .ok_or(ManagerError::UnsupportedWorker)?;
                let pool = usize::try_from(fields.pool)
                    .ok()
                    .filter(|&p| p < 3)
                    .ok_or(ManagerError::UnsupportedWorker)?;
                if self.pools[pool].is_empty() {
                    return Err(ManagerError::UnsupportedWorker.into());
                }
                let mut index = 0;
                if let Some(owner) = self.pools[pool][0].owner {
                    self.worker_enter(effects)?;
                    let last = |owner| {
                        self.records
                            .iter()
                            .find(|r| {
                                r.owner == owner
                                    && Some(r.allocation)
                                        == textures[(owner.slot() - 1) as usize]
                                            .as_ref()
                                            .unwrap()
                                            .managed_record
                                    && self.active.contains(&r.id)
                            })
                            .and_then(|r| r.last_frame)
                    };
                    let mut oldest = last(owner).ok_or(ManagerError::UnsupportedWorker)?;
                    for n in 1..self.pools[pool].len() {
                        match self.pools[pool][n].owner {
                            None => {
                                oldest = 0;
                                index = n;
                                break;
                            }
                            Some(owner) => {
                                let age = last(owner).ok_or(ManagerError::UnsupportedWorker)?;
                                if age < oldest {
                                    oldest = age;
                                    index = n;
                                }
                            }
                        }
                    }
                    let frame = effect!(
                        Frame,
                        effects.worker_effects().unwrap().managed_worker_frame()
                    );
                    if oldest < frame.wrapping_sub(1) {
                        if let Some(owner) = self.pools[pool][index].owner {
                            let t = textures[(owner.slot() - 1) as usize].as_mut().unwrap();
                            t.graphics = None;
                            t.flags &= 0x3fffffff;
                        }
                        self.worker_leave(effects)?;
                    } else {
                        self.worker_leave(effects)?;
                        index = self.worker_grow(
                            pool,
                            fields.format,
                            input.max_slots,
                            input.dimensions[pool],
                            forbidden,
                            effects,
                        )?;
                    }
                }
                let state = self.release.ok_or(ManagerError::WorkerUnavailable)?;
                let system = state.system[pool];
                let surface = system.surface.ok_or(ManagerError::UnsupportedGraphics)?;
                let slot = &self.pools[pool][index];
                let graphics = slot
                    .resources
                    .graphics
                    .ok_or(ManagerError::UnsupportedGraphics)?;
                let desc = ManagedSurfaceDesc(std::array::from_fn(|n| {
                    u32::from_le_bytes(slot.preserved[8 + n * 4..12 + n * 4].try_into().unwrap())
                }));
                let locked = effect!(
                    LockSurface,
                    effects
                        .worker_effects()
                        .unwrap()
                        .lock_managed_surface(surface, 0)
                )
                .filter(|l| l.mapping != 0 && l.writable_bytes >= desc.0[4])
                .ok_or(ManagerError::UnsupportedWorker)?;
                self.worker.as_mut().unwrap().mapping = Some(locked);
                let t = textures[(key.slot() - 1) as usize].as_mut().unwrap();
                t.width = Observed::Value(desc.0[6] as u16);
                t.height = Observed::Value(desc.0[7] as u16);
                let filename = self.records[record]
                    .file_name
                    .ok_or(ManagerError::UnsupportedWorker)?;
                let file = effect!(
                    OpenFile,
                    effects
                        .worker_effects()
                        .unwrap()
                        .open_managed_file(filename, 0)
                )
                .ok_or(ManagerError::UnsupportedWorker)?;
                self.worker.as_mut().unwrap().file = Some(file);
                effect!(
                    SeekFile,
                    effects
                        .worker_effects()
                        .unwrap()
                        .seek_managed_file(file, 0x16, 1)
                );
                effect!(
                    ReadFile,
                    effects
                        .worker_effects()
                        .unwrap()
                        .read_managed_file(file, locked, desc.0[4])
                );
                effect!(
                    CloseFile,
                    effects.worker_effects().unwrap().close_managed_file(file)
                );
                self.worker.as_mut().unwrap().file = None;
                effect!(
                    UnlockSurface,
                    effects
                        .worker_effects()
                        .unwrap()
                        .unlock_managed_surface(surface)
                );
                self.worker.as_mut().unwrap().mapping = None;
                effect!(
                    UpdateTexture,
                    effects.worker_effects().unwrap().update_managed_texture(
                        system.graphics.ok_or(ManagerError::UnsupportedGraphics)?,
                        graphics
                    )
                );
                self.worker_enter(effects)?;
                self.pools[pool][index].owner = Some(key);
                let t = textures[(key.slot() - 1) as usize].as_mut().unwrap();
                t.graphics = Some(graphics);
                t.flags = (t.flags & 0x3fffffff) | 0x80000000;
                // Admitted nonzero preexisting references prove Release's
                // nondeleting branch; the temporary reference is not external.
                effect!(
                    ReleaseWorkerReference,
                    effects
                        .worker_effects()
                        .unwrap()
                        .release_managed_worker_reference(key, t.references, graphics, t.flags)
                );
                t.references -= 1;
                self.worker.as_mut().unwrap().temporary_reference = None;
                self.worker_advance();
                self.worker_leave(effects)?;
                effect!(
                    Sleep,
                    effects.worker_effects().unwrap().managed_worker_sleep(0)
                );
            }
            Err(ManagerError::ScheduleExhausted.into())
        })();
        if let Err(error) = result {
            self.poisoned = true;
            if self.critical == CriticalState::Held {
                let leave = effects.leave_texture_section();
                self.critical = if leave.is_ok() {
                    CriticalState::NotHeld
                } else {
                    CriticalState::LeaveFailed
                };
                return Err(match error {
                    ManagedOperationError::Backend { stage, error, .. } => {
                        ManagedOperationError::Backend {
                            stage,
                            error,
                            leave_error: leave.err(),
                        }
                    }
                    e => e,
                });
            }
            return Err(error);
        }
        result
    }
    #[allow(clippy::too_many_arguments)]
    fn worker_grow<E>(
        &mut self,
        pool: usize,
        format: u32,
        max_slots: u32,
        width: u32,
        forbidden: &[u64],
        effects: &mut dyn ManagedTextureEffects<Error = E>,
    ) -> Result<usize, ManagedOperationError<E>> {
        if effects.restore_effects().is_none() || effects.release_effects().is_none() {
            return Err(ManagerError::WorkerUnavailable.into());
        }
        let index = self.pools[pool].len();
        if index >= max_slots as usize {
            return Err(ManagerError::InvalidCount.into());
        }
        self.pools[pool]
            .try_reserve(1)
            .map_err(|_| ManagerError::HostAllocation)?;
        self.released_borrows
            .try_reserve(self.pools.iter().map(Vec::len).sum::<usize>() + 1)
            .map_err(|_| ManagerError::HostAllocation)?;
        self.allocation_history
            .try_reserve(1)
            .map_err(|_| ManagerError::HostAllocation)?;
        self.next_pool_slot[pool]
            .checked_add(1)
            .ok_or(ManagerError::IdentityOverflow)?;
        macro_rules! effect {
            ($stage:ident,$call:expr) => {{
                self.worker.as_mut().unwrap().stage = ManagedStage::$stage;
                $call.map_err(|error| ManagedOperationError::Backend {
                    stage: ManagedStage::$stage,
                    error,
                    leave_error: None,
                })?
            }};
        }
        let storage = self.release.ok_or(ManagerError::WorkerUnavailable)?.pools[pool];
        if index >= storage.capacity as usize {
            let capacity = storage
                .capacity
                .checked_add(storage.growth)
                .filter(|&n| n > storage.capacity && n <= i32::MAX as u32 / 44)
                .ok_or(ManagerError::InvalidCount)?;
            self.release.as_mut().unwrap().pools[pool].capacity = capacity;
            let allocation = effect!(
                AllocatePool,
                effects
                    .restore_effects()
                    .unwrap()
                    .allocate_managed_pool(capacity * 44)
            )
            .ok_or(ManagerError::UnsupportedAllocation)?;
            self.worker.as_mut().unwrap().pending_pool = Some(allocation);
            if self.aliases(allocation.0, forbidden) {
                return Err(ManagerError::InvalidIdentity.into());
            }
            self.allocation_history.push(allocation.0);
            self.worker.as_mut().unwrap().copied = 0;
            for n in 0..index {
                effect!(
                    CopyPool,
                    effects.restore_effects().unwrap().copy_managed_pool_slot(
                        storage
                            .allocation
                            .ok_or(ManagerError::UnsupportedAllocation)?,
                        allocation,
                        n as u32
                    )
                );
                self.worker.as_mut().unwrap().copied = n as u32 + 1;
            }
            effect!(
                DeletePool,
                effects
                    .release_effects()
                    .unwrap()
                    .delete_managed_pool(storage.allocation)
            );
            self.release.as_mut().unwrap().pools[pool].allocation = Some(allocation);
            self.worker.as_mut().unwrap().pending_pool = None;
        }
        let target = RestoreTarget::Slot {
            pool: pool as u8,
            index: index as u32,
            allocation: self.release.as_ref().unwrap().pools[pool]
                .allocation
                .ok_or(ManagerError::UnsupportedAllocation)?,
        };
        self.pools[pool].push(PoolSlot {
            id: PoolSlotId {
                arena: self.identity,
                pool: pool as u8,
                slot: self.next_pool_slot[pool],
            },
            owner: None,
            preserved: [0; 40],
            resources: PoolResources {
                graphics: None,
                surface: None,
            },
        });
        self.next_pool_slot[pool] += 1;
        let graphics = effect!(
            CreateTexture,
            effects.restore_effects().unwrap().create_managed_texture(
                target,
                ManagedTextureCreate {
                    width,
                    height: width,
                    levels: 1,
                    usage: 0,
                    format,
                    pool: 0
                }
            )
        )
        .filter(|g| g.0 != 0)
        .ok_or(ManagerError::UnsupportedGraphics)?;
        self.pools[pool][index].resources.graphics = Some(graphics);
        let surface = effect!(
            GetSurfaceLevel,
            effects
                .restore_effects()
                .unwrap()
                .get_managed_surface_level(target, graphics, 0)
        )
        .filter(|s| s.0 != 0)
        .ok_or(ManagerError::UnsupportedGraphics)?;
        self.pools[pool][index].resources.surface = Some(surface);
        let desc = effect!(
            GetSurfaceDesc,
            effects
                .restore_effects()
                .unwrap()
                .get_managed_surface_desc(target, surface)
        );
        for (chunk, value) in self.pools[pool][index].preserved[8..]
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(desc.0)
        {
            chunk.copy_from_slice(&value.to_le_bytes());
        }
        effect!(
            WritePoolOwner,
            effects
                .restore_effects()
                .unwrap()
                .clear_managed_pool_owner(target)
        );
        self.release.as_mut().unwrap().pool_memory[pool] =
            self.release.as_ref().unwrap().pool_memory[pool].wrapping_add(desc.0[4]);
        Ok(index)
    }
}
