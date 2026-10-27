//! Host ownership guard around the existing manual CarRuntime lifecycle.
//! Reservations precede opening assets. An interrupted shutdown is never
//! replayed: the same complete runtime allocation remains inspectable on its
//! owning thread. This adapter reconstructs no additional native function.
use super::{CarRuntime, CarRuntimeCleanup, CarRuntimeOpenFailure, CarRuntimeOptions};
use std::{
    cell::RefCell,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    rc::Rc,
};

struct FailedCar {
    runtime: Box<CarRuntime>,
    first_error: String,
}
enum FailureSlot {
    Vacant {
        _last_cleanup: Option<CarRuntimeCleanup>,
    },
    Reserved,
    Failed(Rc<FailedCar>),
}
thread_local! {
    static FAILURE_SLOTS: RefCell<[FailureSlot;16]> = RefCell::new(std::array::from_fn(|_|FailureSlot::Vacant{_last_cleanup:None}));
}
struct Reservation {
    slot: usize,
    active: bool,
    _thread_bound: PhantomData<Rc<()>>,
}
impl Reservation {
    fn new() -> Result<Self, String> {
        let slot = FAILURE_SLOTS.with(|slots| {
            let mut slots = slots.borrow_mut();
            let slot = slots
                .iter()
                .position(|slot| matches!(slot, FailureSlot::Vacant { .. }))
                .ok_or("Car runtime cleanup retention capacity reached")?;
            slots[slot] = FailureSlot::Reserved;
            Ok::<_, String>(slot)
        })?;
        Ok(Self {
            slot,
            active: true,
            _thread_bound: PhantomData,
        })
    }
    fn attach(self, runtime: Box<CarRuntime>) -> GuardedCarRuntime {
        GuardedCarRuntime {
            runtime: Some(runtime),
            reservation: self,
            closed: None,
            _thread_bound: PhantomData,
        }
    }
    fn release(&mut self, cleanup: Option<CarRuntimeCleanup>) {
        if self.active {
            FAILURE_SLOTS.with(|slots| {
                slots.borrow_mut()[self.slot] = FailureSlot::Vacant {
                    _last_cleanup: cleanup,
                }
            });
            self.active = false;
        }
    }
    fn retain(&mut self, runtime: Box<CarRuntime>, first_error: String) {
        FAILURE_SLOTS.with(|slots| {
            slots.borrow_mut()[self.slot] = FailureSlot::Failed(Rc::new(FailedCar {
                runtime,
                first_error,
            }))
        });
        self.active = false;
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        self.release(None);
    }
}

/// The same manually callable runtime, with one reserved host failure slot.
/// Use `close` for guarded teardown. Both this owner and its reserved slot are
/// bound to the creating thread; retained failures live for that thread's life.
pub struct GuardedCarRuntime {
    runtime: Option<Box<CarRuntime>>,
    reservation: Reservation,
    closed: Option<Result<CarRuntimeCleanup, String>>,
    _thread_bound: PhantomData<Rc<()>>,
}
impl Deref for GuardedCarRuntime {
    type Target = CarRuntime;
    fn deref(&self) -> &Self::Target {
        self.runtime.as_deref().expect("live guarded car runtime")
    }
}
impl DerefMut for GuardedCarRuntime {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.runtime
            .as_deref_mut()
            .expect("live guarded car runtime")
    }
}
impl GuardedCarRuntime {
    fn finish_open(
        reservation: Reservation,
        result: Result<CarRuntime, CarRuntimeOpenFailure>,
    ) -> Result<Self, String> {
        match result {
            Ok(runtime) => Ok(reservation.attach(Box::new(runtime))),
            Err(CarRuntimeOpenFailure { message, partial }) => {
                // Move the exact prefix supplied by the manual factory. No
                // resource graph, cache, projection or cleanup state is copied.
                let mut owner = reservation.attach(partial);
                match owner.close() {
                    Ok(_) => Err(message),
                    Err(error) => Err(format!(
                        "{message}; cleanup retained in car runtime slot {}: {error}",
                        owner.reservation.slot
                    )),
                }
            }
        }
    }
    pub fn open(
        installation: lhr_assets::Installation,
        path: &str,
        options: CarRuntimeOptions,
    ) -> Result<Self, String> {
        let reservation = Reservation::new()?;
        Self::finish_open(reservation, CarRuntime::open(installation, path, options))
    }
    pub fn open_driving(
        installation: lhr_assets::Installation,
        path: &str,
        options: CarRuntimeOptions,
        mission: &str,
    ) -> Result<Self, String> {
        let reservation = Reservation::new()?;
        Self::finish_open(
            reservation,
            CarRuntime::open_driving(installation, path, options, mission),
        )
    }
    pub fn open_driving_scene(
        installation: lhr_assets::Installation,
        path: &str,
        options: CarRuntimeOptions,
        mission: &str,
    ) -> Result<Self, String> {
        let reservation = Reservation::new()?;
        Self::finish_open(
            reservation,
            CarRuntime::open_driving_scene(installation, path, options, mission),
        )
    }
    pub fn open_driving_scene_with_patches(
        installation: lhr_assets::Installation,
        path: &str,
        options: CarRuntimeOptions,
        mission: &str,
        patches: &crate::program_patch::ProgramPatchLease,
    ) -> Result<Self, String> {
        let reservation = Reservation::new()?;
        Self::finish_open(
            reservation,
            CarRuntime::open_driving_scene_with_patches(
                installation,
                path,
                options,
                mission,
                patches,
            ),
        )
    }
    /// Calls the existing shutdown once. Repeated success returns its report;
    /// repeated failure returns the first error and leaves the prefix intact.
    pub fn close(&mut self) -> Result<CarRuntimeCleanup, String> {
        if let Some(result) = &self.closed {
            return result.clone();
        }
        let result = self
            .runtime
            .as_mut()
            .expect("live guarded car runtime")
            .shutdown();
        if let Ok(cleanup) = &result {
            self.reservation.release(Some(cleanup.clone()));
        }
        self.closed = Some(result.clone());
        result
    }
}
impl Drop for GuardedCarRuntime {
    fn drop(&mut self) {
        if self.runtime.is_none() {
            return;
        }
        if let Err(first_error) = self.close() {
            let runtime = self.runtime.take().unwrap();
            self.reservation.retain(runtime, first_error);
        }
    }
}
/// Inspect a failed owner without exposing mutable runtime access. A temporary
/// host reference releases the TLS borrow before invoking the callback, so it
/// is safe to inspect slots while another independent guard is dropped.
pub fn inspect_failed_car<R>(
    slot: usize,
    inspect: impl FnOnce(&CarRuntime, &str) -> R,
) -> Option<R> {
    let failed = FAILURE_SLOTS.with(|slots| match slots.borrow().get(slot) {
        Some(FailureSlot::Failed(failed)) => Some(Rc::clone(failed)),
        _ => None,
    })?;
    Some(inspect(&failed.runtime, &failed.first_error))
}
pub fn failure_slots() -> Vec<usize> {
    FAILURE_SLOTS.with(|slots| {
        slots
            .borrow()
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| matches!(slot, FailureSlot::Failed(_)).then_some(index))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        mission_resources::{
            CpuMissionCacheMemory, MissionCacheKind, MissionCaches, ModelCachePhase,
        },
        mission_scene_loading::MissionSceneLoader,
    };
    use std::{
        path::{Path, PathBuf},
        sync::Arc,
    };
    fn directory() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "lhr-car-owner-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir(&path).unwrap();
        path
    }
    fn missing_prefix(directory: &Path) -> Box<CarRuntime> {
        match CarRuntime::open(
            lhr_assets::Installation::open(directory).unwrap(),
            "models/missing.4ds",
            CarRuntimeOptions::default(),
        ) {
            Ok(_) => panic!("missing synthetic car unexpectedly opened"),
            Err(failure) => failure.partial,
        }
    }
    fn write_dummy(directory: &Path) {
        std::fs::create_dir(directory.join("models")).unwrap();
        let mut bytes = b"4DS\0".to_vec();
        bytes.extend(29u16.to_le_bytes());
        bytes.extend([0; 8]);
        bytes.extend(0u16.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.push(6);
        bytes.extend(0u16.to_le_bytes());
        for value in [0f32, 0., 0., 1., 1., 1., 1., 0., 0., 0.] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend([0x11, 4]);
        bytes.extend(b"part");
        bytes.push(0);
        bytes.extend([0; 24]);
        bytes.push(0);
        std::fs::write(directory.join("models/dummy.4ds"), bytes).unwrap();
    }
    fn startup_source(directory: &Path) -> crate::program_patch::ProgramPatchStartup {
        std::fs::create_dir_all(directory.join("patch")).unwrap();
        let mut payload = Vec::new();
        for value in [
            Some(b"synthetic\0".as_slice()),
            Some(b"retained\0".as_slice()),
            None,
            Some(b"startup car source\0".as_slice()),
        ] {
            payload.extend((value.map_or(0, |value| value.len()) as i32).to_le_bytes());
            if let Some(value) = value {
                payload.extend(value.iter().map(|byte| byte ^ 0x85));
            }
        }
        std::fs::write(
            directory.join("patch/a4.bin"),
            crate::original_data_chunk::chunk(0x64, &payload),
        )
        .unwrap();
        let mut startup = crate::program_patch::ProgramPatchStartup::default();
        startup
            .initialize(&lhr_assets::Installation::open(directory).unwrap())
            .unwrap();
        startup
    }
    #[test]
    fn driving_source_loader_borrows_actual_startup_table_until_successful_close() {
        use crate::program_patch::ProgramPatchStartupError;
        let directory = directory();
        let mut startup = startup_source(&directory);
        std::fs::create_dir_all(directory.join("missions/synthetic")).unwrap();
        let chunk = crate::original_data_chunk::chunk;
        let program = chunk(
            0xae51,
            &[
                vec![0],
                8u32.to_le_bytes().to_vec(),
                b"retained".to_vec(),
                8u32.to_le_bytes().to_vec(),
                b"authored".to_vec(),
            ]
            .concat(),
        );
        std::fs::write(
            directory.join("missions/synthetic/scene2.bin"),
            chunk(
                0x4c53,
                &[chunk(1, &[5, 0, 1, 0]), chunk(0xae50, &program)].concat(),
            ),
        )
        .unwrap();
        let mut runtime = missing_prefix(&directory);
        runtime.startup_patches = Some(startup.lease().unwrap());
        runtime
            .load_driving_scene2("synthetic", CarRuntimeOptions::default())
            .unwrap();
        let content = &runtime.scene_mission.as_ref().unwrap().loader.content;
        assert_eq!(content.mission_path(), Some(b"synthetic".as_slice()));
        assert_eq!(
            content.programs()[0].source().unwrap().c_bytes(),
            Some(b"startup car source".as_slice())
        );
        assert_eq!(
            startup.unload_after_sources(),
            Err(ProgramPatchStartupError::SourcesRetainTable)
        );
        let mut owner = Reservation::new().unwrap().attach(runtime);
        owner.close().unwrap();
        assert!(owner.startup_patches.is_none());
        startup.unload_after_sources().unwrap();
        assert!(startup.table().replacements().is_empty());
        drop(owner);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn missing_asset_prefix_auto_closes_real_scene_and_reuses_its_slot() {
        let directory = directory();
        let runtime = missing_prefix(&directory);
        assert!(runtime.scene().is_some());
        assert!(runtime.arena().live_frame_count() > 0);
        let reservation = Reservation::new().unwrap();
        let slot = reservation.slot;
        drop(reservation.attach(runtime));
        FAILURE_SLOTS.with(|slots| {
            let slots = slots.borrow();
            let FailureSlot::Vacant {
                _last_cleanup: Some(cleanup),
            } = &slots[slot]
            else {
                panic!("actual shutdown did not finish")
            };
            assert_eq!(cleanup.heap_allocations, 0);
            assert_eq!(cleanup.graphics, 0);
            assert_eq!(cleanup.textures, 0);
            assert_eq!(cleanup.cpu_allocations, 0);
            assert_eq!(cleanup.retained_scene_prefix_frames, 0);
            assert_eq!(cleanup.driver_scene_registrations, 0);
            assert_eq!(cleanup.driver_scene_capacity, 0);
        });
        let reused = Reservation::new().unwrap();
        assert_eq!(reused.slot, slot);
        drop(reused);
        assert!(!failure_slots().contains(&slot));
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn interrupted_real_cache_owner_survives_close_error_and_drop_without_replay() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let directory = directory();
            write_dummy(&directory);
            let mut startup = startup_source(&directory);
            let lease = startup.lease().unwrap();
            let weak_table = lease.weak_table();
            let mut runtime = missing_prefix(&directory);
            runtime.startup_patches = Some(lease);
            let scene = runtime.scene().unwrap();
            let source = runtime.arena.create_model().unwrap();
            let dummy = crate::model_loading::tests::frame(6, None, [0.; 3]);
            runtime
                .arena
                .load_parsed_into_model(
                    source,
                    Arc::new(lhr_assets::Model {
                        materials: vec![],
                        frames: vec![dummy],
                    }),
                )
                .unwrap();
            runtime
                .arena
                .load_plain_visual_resources(source, &runtime.installation)
                .unwrap();
            let source_frame = runtime.arena.model(source).unwrap().frames[0];
            let mut caches = MissionCaches::empty().unwrap();
            let mut memory = CpuMissionCacheMemory::default();
            caches
                .add_dir(MissionCacheKind::Model, b"Models", &mut memory)
                .unwrap();
            caches
                .retain_model(source, b"fixture.i3d", &mut runtime.arena, &mut memory)
                .unwrap();
            let target = runtime.arena.create_model().unwrap();
            assert_eq!(
                caches
                    .open_static_model(
                        &mut runtime.arena,
                        &runtime.installation,
                        target,
                        b"dummy.i3d",
                        &mut memory
                    )
                    .unwrap(),
                0
            );
            runtime.arena.release(target.root()).unwrap();
            runtime.arena.release(source.root()).unwrap();
            runtime
                .arena
                .graph
                .assign_scale(source_frame, [4.; 3])
                .unwrap();
            let target = runtime.arena.create_model().unwrap();
            assert!(caches
                .open_static_model(
                    &mut runtime.arena,
                    &runtime.installation,
                    target,
                    b"fixture.i3d",
                    &mut memory
                )
                .is_err());
            assert!(caches.is_interrupted(MissionCacheKind::Model));
            assert_eq!(
                caches.model_open_progress().unwrap().phase,
                ModelCachePhase::DuplicateHit
            );
            assert_eq!(
                caches.model_open_progress().unwrap().cached_source,
                Some(source)
            );
            runtime.arena.release(target.root()).unwrap();
            let source_reference = runtime.arena.lookup_reference(source.root());
            let bytes = memory.live_bytes();
            let allocations = memory.live_allocations();
            assert!(bytes > 0 && allocations > 0);
            let progress = format!("{:?}", caches.model_open_progress());
            runtime.scene_mission = Some(super::super::scene_loading::DrivingSceneMission {
                loader: MissionSceneLoader::new(&runtime.arena, scene)
                    .unwrap_or_else(|error| panic!("{}", error.detail)),
                frontier: None,
                caches,
                memory,
            });
            let pointer = (&*runtime as *const CarRuntime) as usize;
            let reservation = Reservation::new().unwrap();
            let slot = reservation.slot;
            let mut owner = reservation.attach(runtime);
            let first = owner.close().unwrap_err();
            assert!(first.contains("Driving scene cache cleanup"));
            assert!(owner.scene().is_none());
            assert_eq!(
                owner.arena.lookup_reference(source.root()),
                source_reference
            );
            assert_eq!(owner.close().unwrap_err(), first);
            assert!(owner.shutdown_started);
            assert!(!owner.game_done.has_started());
            assert_eq!(
                startup.unload_after_sources(),
                Err(crate::program_patch::ProgramPatchStartupError::SourcesRetainTable)
            );
            assert_eq!(startup.table().replacements().len(), 1);
            drop(startup);
            drop(owner);
            assert!(failure_slots().contains(&slot));
            assert_eq!(
                weak_table
                    .upgrade()
                    .expect("failed car retains actual startup data")
                    .replacements()
                    .len(),
                1
            );
            inspect_failed_car(slot, |runtime, error| {
                assert_eq!((runtime as *const CarRuntime) as usize, pointer);
                assert_eq!(error, first);
                assert_eq!(
                    runtime.arena.lookup_reference(source.root()),
                    source_reference
                );
                assert_eq!(runtime.arena.frame(source.root()).unwrap().references, 1);
                let mission = runtime.scene_mission.as_ref().unwrap();
                assert!(mission.caches.is_interrupted(MissionCacheKind::Model));
                assert_eq!(mission.memory.live_bytes(), bytes);
                assert_eq!(mission.memory.live_allocations(), allocations);
                assert_eq!(
                    format!("{:?}", mission.caches.model_open_progress()),
                    progress
                );
            })
            .unwrap();
            std::fs::remove_dir_all(directory).unwrap();
        });
    }
    #[test]
    fn reservation_capacity_rejects_before_open_and_unused_reservations_release() {
        let available = 16 - failure_slots().len();
        let reservations = (0..available)
            .map(|_| Reservation::new().unwrap())
            .collect::<Vec<_>>();
        assert!(Reservation::new().is_err());
        drop(reservations);
        assert!(Reservation::new().is_ok());
    }
}
