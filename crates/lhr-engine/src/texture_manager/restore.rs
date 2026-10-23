//! RestoreManagedTextures is an upstream working name, PC RVA0x7a710.
//! Required serialized device/storage effects; no GPU or worker implementation.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManagedTextureCreate {
    pub width: u32,
    pub height: u32,
    pub levels: u32,
    pub usage: u32,
    pub format: u32,
    pub pool: u32,
}
/// D3D8 surface descriptor in native dword order. Size is dword four.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManagedSurfaceDesc(pub [u32; 8]);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestoreTarget {
    System(u8),
    Slot {
        pool: u8,
        index: u32,
        allocation: ManagedPoolAllocation,
    },
}
/// All effects preserve identity domain and cannot reenter. Successful COM
/// outputs transfer one owned reference, including repeated COM aliases, and
/// must support the corresponding Release capability for its full lifetime.
/// Native HRESULTs are ignored: valid output even on failing HRESULT continues;
/// Rust Err represents host interruption, never a native HRESULT branch.
/// Err may have performed an effect: interruption poisons without rollback.
/// None models an unsupported unchecked native null output, not HRESULT logic.
/// Backend writes outputs to the supplied target's native storage before return.
pub trait ManagedTextureRestoreEffects {
    type Error;
    fn create_managed_texture(
        &mut self,
        target: RestoreTarget,
        request: ManagedTextureCreate,
    ) -> Result<Option<TextureGraphics>, Self::Error>;
    fn get_managed_surface_level(
        &mut self,
        target: RestoreTarget,
        texture: TextureGraphics,
        level: u32,
    ) -> Result<Option<TextureSurface>, Self::Error>;
    fn get_managed_surface_desc(
        &mut self,
        target: RestoreTarget,
        surface: TextureSurface,
    ) -> Result<ManagedSurfaceDesc, Self::Error>;
    fn allocate_managed_pool(
        &mut self,
        bytes: u32,
    ) -> Result<Option<ManagedPoolAllocation>, Self::Error>;
    /// Copies all eleven dwords, preserving the owned references without AddRef.
    fn copy_managed_pool_slot(
        &mut self,
        from: ManagedPoolAllocation,
        to: ManagedPoolAllocation,
        index: u32,
    ) -> Result<(), Self::Error>;
    fn clear_managed_pool_owner(&mut self, target: RestoreTarget) -> Result<(), Self::Error>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RestoreInput {
    /// Native ECX flags, including format selector bit0x80000.
    pub flags: u32,
    pub formats: CreationFormats,
    pub dimensions: [u32; 3],
    pub budgets: [i32; 3],
    /// Host bound on each pool's finite reconstruction. Exhaustion rejects and
    /// poisons the completed prefix; no original allocation failure recovery.
    pub max_slots: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoreProgress {
    pub pool: u8,
    pub stage: ManagedStage,
    pub pending_pool: Option<ManagedPoolAllocation>,
    pub pending_registry: Option<ManagedRegistryAllocation>,
    pub copied: u32,
    pub system_desc: [Option<ManagedSurfaceDesc>; 3],
    pub slot_desc: [Vec<ManagedSurfaceDesc>; 3],
    pub complete: bool,
}
impl TextureManager {
    pub fn restore_progress(&self) -> Option<&RestoreProgress> {
        self.restore.as_ref()
    }
    pub(crate) fn restore<E>(
        &mut self,
        input: RestoreInput,
        forbidden: &[u64],
        clear_loaded: impl FnMut(TextureKey),
        effects: &mut dyn ManagedTextureEffects<Error = E>,
    ) -> Result<(), ManagedOperationError<E>> {
        if self.poisoned {
            return Err(ManagerError::Poisoned.into());
        }
        if self.registry.is_none()
            || self.release.is_none()
            || effects.restore_effects().is_none()
            || effects.creation_effects().is_none()
        {
            return Err(ManagerError::RestoreUnavailable.into());
        }
        // Bounded host domain excludes signed overflow and unsafe native extents.
        if input.max_slots > i32::MAX as u32 / 44
            || input
                .max_slots
                .checked_add(0x3ff)
                .and_then(|n| (n & !0x3ff).checked_mul(44))
                .is_none()
        {
            return Err(ManagerError::InvalidCount.into());
        }
        for i in 0..3 {
            self.next_pool_slot[i]
                .checked_add(input.max_slots)
                .ok_or(ManagerError::IdentityOverflow)?;
            self.pools[i]
                .try_reserve(input.max_slots as usize)
                .map_err(|_| ManagerError::HostAllocation)?;
        }
        self.allocation_history
            .try_reserve((input.max_slots as usize / 0x400 + 1) * 3 + 1)
            .map_err(|_| ManagerError::HostAllocation)?;
        self.released_borrows
            .try_reserve(
                input.max_slots as usize * 3 + self.pools.iter().map(Vec::len).sum::<usize>(),
            )
            .map_err(|_| ManagerError::HostAllocation)?;
        let mut progress = RestoreProgress {
            pool: 0,
            stage: ManagedStage::CreateTexture,
            pending_pool: None,
            pending_registry: None,
            copied: 0,
            system_desc: [None; 3],
            slot_desc: std::array::from_fn(|_| Vec::new()),
            complete: false,
        };
        for desc in &mut progress.slot_desc {
            if desc.try_reserve(input.max_slots as usize).is_err() {
                return Err(ManagerError::HostAllocation.into());
            }
        }
        self.release(clear_loaded, effects)?;
        self.restore = Some(progress);
        let format = if input.flags & 0x80000 != 0 && input.formats.dxt != 0 {
            input.formats.dxt
        } else {
            input.formats.normal
        };
        macro_rules! effect {
            ($stage:ident, $call:expr) => {{
                self.restore.as_mut().unwrap().stage = ManagedStage::$stage;
                $call.map_err(|error| ManagedOperationError::Backend {
                    stage: ManagedStage::$stage,
                    error,
                    leave_error: None,
                })?
            }};
        }
        let result = (|| {
            for i in 0..3 {
                self.restore.as_mut().unwrap().pool = i as u8;
                self.release.as_mut().unwrap().pools[i].growth = 0x400;
                let request = ManagedTextureCreate {
                    width: input.dimensions[i],
                    height: input.dimensions[i],
                    levels: 1,
                    usage: 0,
                    format,
                    pool: 2,
                };
                let target = RestoreTarget::System(i as u8);
                let g = effect!(
                    CreateTexture,
                    effects
                        .restore_effects()
                        .unwrap()
                        .create_managed_texture(target, request)
                )
                .filter(|g| g.0 != 0)
                .ok_or(ManagerError::UnsupportedGraphics)?;
                self.release.as_mut().unwrap().system[i].graphics = Some(g);
                let s = effect!(
                    GetSurfaceLevel,
                    effects
                        .restore_effects()
                        .unwrap()
                        .get_managed_surface_level(target, g, 0)
                )
                .filter(|s| s.0 != 0)
                .ok_or(ManagerError::UnsupportedGraphics)?;
                self.release.as_mut().unwrap().system[i].surface = Some(s);
                let desc = effect!(
                    GetSurfaceDesc,
                    effects
                        .restore_effects()
                        .unwrap()
                        .get_managed_surface_desc(target, s)
                );
                self.restore.as_mut().unwrap().system_desc[i] = Some(desc);
                effect!(
                    WritePoolOwner,
                    effects
                        .restore_effects()
                        .unwrap()
                        .clear_managed_pool_owner(target)
                );
                while input.budgets[i] > self.release.as_ref().unwrap().pool_memory[i] as i32 {
                    let index = self.pools[i].len() as u32;
                    if index >= input.max_slots {
                        return Err(ManagerError::InvalidCount.into());
                    }
                    let storage = self.release.as_ref().unwrap().pools[i];
                    if index >= storage.capacity {
                        let cap = storage
                            .capacity
                            .checked_add(0x400)
                            .filter(|&n| n <= i32::MAX as u32)
                            .ok_or(ManagerError::InvalidCount)?;
                        let bytes = cap.checked_mul(44).ok_or(ManagerError::InvalidCount)?;
                        self.release.as_mut().unwrap().pools[i].capacity = cap;
                        let allocation = effect!(
                            AllocatePool,
                            effects
                                .restore_effects()
                                .unwrap()
                                .allocate_managed_pool(bytes)
                        )
                        .ok_or(ManagerError::UnsupportedAllocation)?;
                        self.restore.as_mut().unwrap().pending_pool = Some(allocation);
                        if self.aliases(allocation.0, forbidden) {
                            return Err(ManagerError::InvalidIdentity.into());
                        }
                        self.allocation_history.push(allocation.0);
                        self.restore.as_mut().unwrap().copied = 0;
                        for n in 0..index {
                            effect!(
                                CopyPool,
                                effects.restore_effects().unwrap().copy_managed_pool_slot(
                                    storage.allocation.unwrap(),
                                    allocation,
                                    n
                                )
                            );
                            self.restore.as_mut().unwrap().copied = n + 1;
                        }
                        effect!(
                            DeletePool,
                            effects
                                .release_effects()
                                .unwrap()
                                .delete_managed_pool(storage.allocation)
                        );
                        self.release.as_mut().unwrap().pools[i].allocation = Some(allocation);
                        self.restore.as_mut().unwrap().pending_pool = None;
                    }
                    let target = RestoreTarget::Slot {
                        pool: i as u8,
                        index,
                        allocation: self.release.as_ref().unwrap().pools[i].allocation.unwrap(),
                    };
                    // Native count publication precedes every graphics effect.
                    // None/zero fields are host placeholders, not initialized
                    // native bytes. Backend storage remains untouched until
                    // each explicit output/owner callback; copies retain all44 bytes.
                    self.pools[i].push(PoolSlot {
                        id: PoolSlotId {
                            arena: self.identity,
                            pool: i as u8,
                            slot: self.next_pool_slot[i],
                        },
                        owner: None,
                        preserved: [0; 40],
                        resources: PoolResources {
                            graphics: None,
                            surface: None,
                        },
                    });
                    self.next_pool_slot[i] += 1;
                    let g = effect!(
                        CreateTexture,
                        effects.restore_effects().unwrap().create_managed_texture(
                            target,
                            ManagedTextureCreate { pool: 0, ..request }
                        )
                    )
                    .filter(|g| g.0 != 0)
                    .ok_or(ManagerError::UnsupportedGraphics)?;
                    self.pools[i][index as usize].resources.graphics = Some(g);
                    let s = effect!(
                        GetSurfaceLevel,
                        effects
                            .restore_effects()
                            .unwrap()
                            .get_managed_surface_level(target, g, 0)
                    )
                    .filter(|s| s.0 != 0)
                    .ok_or(ManagerError::UnsupportedGraphics)?;
                    self.pools[i][index as usize].resources.surface = Some(s);
                    let desc = effect!(
                        GetSurfaceDesc,
                        effects
                            .restore_effects()
                            .unwrap()
                            .get_managed_surface_desc(target, s)
                    );
                    self.restore.as_mut().unwrap().slot_desc[i].push(desc);
                    for (chunk, value) in self.pools[i][index as usize].preserved[8..]
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
                    let memory =
                        self.release.as_ref().unwrap().pool_memory[i].wrapping_add(desc.0[4]);
                    self.release.as_mut().unwrap().pool_memory[i] = memory;
                }
            }
            let registry = self.registry.unwrap();
            let capacity = (self.active.len() as u32).max(0x1000);
            if registry.capacity < capacity {
                let bytes = capacity.checked_mul(4).ok_or(ManagerError::InvalidCount)?;
                self.registry.as_mut().unwrap().capacity = capacity;
                let array = effect!(
                    AllocateRegistry,
                    effects
                        .creation_effects()
                        .unwrap()
                        .allocate_managed_registry(bytes)
                )
                .ok_or(ManagerError::UnsupportedAllocation)?;
                self.restore.as_mut().unwrap().pending_registry = Some(array);
                if self.aliases(array.0, forbidden) {
                    return Err(ManagerError::InvalidIdentity.into());
                }
                self.allocation_history.push(array.0);
                self.restore.as_mut().unwrap().copied = 0;
                for (index, id) in self.active.iter().enumerate() {
                    effect!(
                        CopyRegistry,
                        effects
                            .creation_effects()
                            .unwrap()
                            .write_managed_registry_entry(
                                array,
                                index as u32,
                                self.records[(id.slot - 1) as usize].allocation
                            )
                    );
                    self.restore.as_mut().unwrap().copied = index as u32 + 1;
                }
                effect!(
                    DeleteRegistry,
                    effects
                        .creation_effects()
                        .unwrap()
                        .delete_managed_registry(registry.allocation)
                );
                self.registry.as_mut().unwrap().allocation = Some(array);
                self.restore.as_mut().unwrap().pending_registry = None;
            }
            self.restore.as_mut().unwrap().complete = true;
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    #[derive(Default)]
    struct Effects {
        next: u64,
        log: Vec<String>,
        pools: BTreeSet<u64>,
        arrays: BTreeSet<u64>,
        fail: Option<ManagedStage>,
        alias: bool,
        null_slot: bool,
    }
    impl Effects {
        fn id(&mut self) -> u64 {
            self.next += 1;
            self.next
        }
        fn event(&mut self, s: ManagedStage, text: String) -> Result<(), &'static str> {
            self.log.push(text);
            if self.fail == Some(s) {
                Err("interrupted")
            } else {
                Ok(())
            }
        }
    }
    impl ManagedTextureEffects for Effects {
        type Error = &'static str;
        fn restore_effects(
            &mut self,
        ) -> Option<&mut dyn ManagedTextureRestoreEffects<Error = Self::Error>> {
            Some(self)
        }
        fn creation_effects(
            &mut self,
        ) -> Option<&mut dyn ManagedTextureCreationEffects<Error = Self::Error>> {
            Some(self)
        }
        fn release_effects(
            &mut self,
        ) -> Option<&mut dyn ManagedTextureReleaseEffects<Error = Self::Error>> {
            Some(self)
        }
        fn enter_texture_section(&mut self) -> Result<(), Self::Error> {
            self.log.push("enter".into());
            Ok(())
        }
        fn leave_texture_section(&mut self) -> Result<(), Self::Error> {
            self.log.push("leave".into());
            Ok(())
        }
        fn delete_managed_filename(
            &mut self,
            _: Option<ManagedFilenameAllocation>,
        ) -> Result<(), Self::Error> {
            unreachable!()
        }
        fn delete_managed_record(&mut self, _: ManagedRecordKey) -> Result<(), Self::Error> {
            unreachable!()
        }
    }
    impl ManagedTextureReleaseEffects for Effects {
        type Error = &'static str;
        fn can_release_surface(&self, _: TextureSurface) -> bool {
            true
        }
        fn can_release_texture_graphics(&self, _: TextureGraphics) -> bool {
            true
        }
        fn can_delete_managed_pool(&self, a: Option<ManagedPoolAllocation>) -> bool {
            a.is_none_or(|a| self.pools.contains(&a.0))
        }
        fn release_surface(&mut self, s: TextureSurface) -> Result<(), Self::Error> {
            self.log.push(format!("release surface {}", s.0));
            Ok(())
        }
        fn release_texture_graphics(&mut self, g: TextureGraphics) -> Result<(), Self::Error> {
            self.log.push(format!("release graphics {}", g.0));
            Ok(())
        }
        fn delete_managed_pool(
            &mut self,
            a: Option<ManagedPoolAllocation>,
        ) -> Result<(), Self::Error> {
            self.event(ManagedStage::DeletePool, format!("free pool {a:?}"))?;
            if let Some(a) = a {
                assert!(self.pools.remove(&a.0));
            }
            Ok(())
        }
    }
    impl ManagedTextureCreationEffects for Effects {
        type Error = &'static str;
        fn allocate_managed_record(
            &mut self,
            _: u32,
        ) -> Result<Option<ManagedRecordKey>, Self::Error> {
            unreachable!()
        }
        fn write_managed_record(
            &mut self,
            _: ManagedRecordKey,
            _: RecordWrite,
        ) -> Result<(), Self::Error> {
            unreachable!()
        }
        fn allocate_managed_filename(
            &mut self,
            _: u32,
        ) -> Result<Option<ManagedFilenameAllocation>, Self::Error> {
            unreachable!()
        }
        fn copy_managed_filename(
            &mut self,
            _: ManagedFilenameAllocation,
            _: &[u8],
        ) -> Result<(), Self::Error> {
            unreachable!()
        }
        fn allocate_managed_registry(
            &mut self,
            n: u32,
        ) -> Result<Option<ManagedRegistryAllocation>, Self::Error> {
            self.event(ManagedStage::AllocateRegistry, format!("registry {n}"))?;
            let id = self.id();
            self.arrays.insert(id);
            Ok(Some(ManagedRegistryAllocation(id)))
        }
        fn write_managed_registry_entry(
            &mut self,
            _: ManagedRegistryAllocation,
            _: u32,
            _: ManagedRecordKey,
        ) -> Result<(), Self::Error> {
            Ok(())
        }
        fn delete_managed_registry(
            &mut self,
            a: Option<ManagedRegistryAllocation>,
        ) -> Result<(), Self::Error> {
            if let Some(a) = a {
                assert!(self.arrays.remove(&a.0));
            }
            Ok(())
        }
        fn managed_texture_low_memory(&mut self) -> Result<(), Self::Error> {
            unreachable!()
        }
    }
    impl ManagedTextureRestoreEffects for Effects {
        type Error = &'static str;
        fn create_managed_texture(
            &mut self,
            t: RestoreTarget,
            r: ManagedTextureCreate,
        ) -> Result<Option<TextureGraphics>, Self::Error> {
            self.event(
                ManagedStage::CreateTexture,
                format!("create {t:?} {} {}", r.pool, r.format),
            )?;
            assert_eq!((r.width, r.height, r.levels, r.usage), (16, 16, 1, 0));
            if self.null_slot && matches!(t, RestoreTarget::Slot { .. }) {
                return Ok(None);
            }
            let id = self.id();
            Ok(Some(TextureGraphics(if self.alias { 999 } else { id })))
        }
        fn get_managed_surface_level(
            &mut self,
            _: RestoreTarget,
            _: TextureGraphics,
            l: u32,
        ) -> Result<Option<TextureSurface>, Self::Error> {
            assert_eq!(l, 0);
            self.event(ManagedStage::GetSurfaceLevel, "surface".into())?;
            Ok(Some(TextureSurface(self.id())))
        }
        fn get_managed_surface_desc(
            &mut self,
            _: RestoreTarget,
            _: TextureSurface,
        ) -> Result<ManagedSurfaceDesc, Self::Error> {
            self.event(ManagedStage::GetSurfaceDesc, "desc".into())?;
            Ok(ManagedSurfaceDesc([21, 1, 0, 0, 64, 0, 16, 16]))
        }
        fn allocate_managed_pool(
            &mut self,
            n: u32,
        ) -> Result<Option<ManagedPoolAllocation>, Self::Error> {
            self.event(ManagedStage::AllocatePool, format!("pool {n}"))?;
            let id = self.id();
            self.pools.insert(id);
            Ok(Some(ManagedPoolAllocation(id)))
        }
        fn copy_managed_pool_slot(
            &mut self,
            a: ManagedPoolAllocation,
            b: ManagedPoolAllocation,
            n: u32,
        ) -> Result<(), Self::Error> {
            assert!(self.pools.contains(&a.0) && self.pools.contains(&b.0));
            self.event(ManagedStage::CopyPool, format!("copy {n}"))
        }
        fn clear_managed_pool_owner(&mut self, t: RestoreTarget) -> Result<(), Self::Error> {
            self.event(ManagedStage::WritePoolOwner, format!("owner {t:?}"))
        }
    }
    fn fixture() -> (TextureManager, Effects, RestoreInput) {
        let mut m = TextureManager::new(1);
        m.install(
            ManagedWorldIntake {
                records: vec![],
                queue: [None; 256],
                counters: QueueCounters::default(),
                pools: std::array::from_fn(|_| vec![]),
                bindings: vec![],
            },
            false,
        )
        .unwrap();
        m.configure_registry(
            RegistryIntake {
                capacity: 0,
                growth: 1,
                allocation: None,
            },
            &[],
        )
        .unwrap();
        m.configure_release(
            ManagedReleaseIntake {
                system: [PoolResources {
                    graphics: None,
                    surface: None,
                }; 3],
                pools: [ManagedPoolStorage {
                    allocation: None,
                    capacity: 0,
                    growth: 0,
                }; 3],
                pool_memory: [0; 3],
            },
            &[],
        )
        .unwrap();
        (
            m,
            Effects::default(),
            RestoreInput {
                flags: 0x80000,
                formats: CreationFormats {
                    dxt: 0x31545844,
                    normal: 21,
                },
                dimensions: [16; 3],
                budgets: [1, 65, 129],
                max_slots: 1025,
            },
        )
    }
    #[test]
    fn restore_growth_ownership_and_second_lifecycle() {
        let (mut m, mut e, mut input) = fixture();
        e.alias = true;
        input.budgets = [65600, 0, 0];
        m.restore(input, &[], |_| panic!(), &mut e).unwrap();
        assert_eq!(m.pools().each_ref().map(|p| p.len()), [1025, 0, 0]);
        assert_eq!(m.release_state().unwrap().pool_memory, [65600, 0, 0]);
        assert_eq!(m.release_state().unwrap().pools[0].capacity, 2048);
        assert_eq!(
            e.log.iter().filter(|s| s.starts_with("copy ")).count(),
            1024
        );
        assert!(e.log[0] == "enter" && e.log[1] == "leave");
        let old = m.pools()[0][0].id;
        m.release(|_| panic!(), &mut e).unwrap();
        assert_eq!(
            e.log
                .iter()
                .filter(|s| *s == "release graphics 999")
                .count(),
            1028
        );
        input.budgets = [1, 65, 129];
        m.restore(input, &[], |_| panic!(), &mut e).unwrap();
        assert_eq!(m.pools().each_ref().map(|p| p.len()), [1, 2, 3]);
        assert_eq!(m.pool_slot(old), Err(ManagerError::UnknownPool));
        assert!(m.restore_progress().unwrap().complete);
        m.release(|_| panic!(), &mut e).unwrap();
        assert!(e.pools.is_empty());
    }
    #[test]
    fn unchecked_null_slot_preserves_published_count_and_owned_system_prefix() {
        let (mut m, mut e, input) = fixture();
        e.null_slot = true;
        assert!(matches!(
            m.restore(input, &[], |_| panic!(), &mut e),
            Err(ManagedOperationError::State(
                ManagerError::UnsupportedGraphics
            ))
        ));
        assert_eq!(m.pools()[0].len(), 1);
        assert_eq!(m.pools()[0][0].resources.graphics, None);
        assert!(m.release_state().unwrap().system[0].surface.is_some());
        assert_eq!(m.release_state().unwrap().pool_memory[0], 0);
        assert!(m.restore_progress().unwrap().slot_desc[0].is_empty());
        assert!(!m.restore_progress().unwrap().complete);
    }
    #[test]
    fn failures_keep_published_capacity_count_and_poison_without_retry() {
        for stage in [
            ManagedStage::AllocatePool,
            ManagedStage::GetSurfaceLevel,
            ManagedStage::GetSurfaceDesc,
            ManagedStage::WritePoolOwner,
            ManagedStage::AllocateRegistry,
        ] {
            let (mut m, mut e, mut input) = fixture();
            e.fail = Some(stage);
            if stage == ManagedStage::AllocateRegistry {
                input.budgets = [0; 3];
            }
            assert!(m.restore(input, &[], |_| panic!(), &mut e).is_err());
            if stage == ManagedStage::AllocatePool {
                assert_eq!(m.release_state().unwrap().pools[0].capacity, 1024);
                assert!(m.pools()[0].is_empty());
            }
            if stage == ManagedStage::AllocateRegistry {
                assert_eq!(m.registry().unwrap().capacity, 4096);
                assert_eq!(m.registry().unwrap().allocation, None);
            }
            let log = e.log.clone();
            assert!(matches!(
                m.restore(input, &[], |_| panic!(), &mut e),
                Err(ManagedOperationError::State(ManagerError::Poisoned))
            ));
            assert_eq!(e.log, log);
        }
    }
}
