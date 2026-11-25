//! The ordered CarTables span of PC C_game::Init, after segments.Load.
//!
//! Primary static REA/Ghidra: Game.exe SHA-256
//! 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27,
//! Init RVA 0x1a0810, instructions RVA 0x1a0f78..0x1a0f92
//! (ev_67ab357fe5f281740e780f4e3b413abe2cfe2da884f45c533a229c3b1ea9f020).
//! Reference: mafia-re fe7e74643f5415c358aa312dd4e7d9da5f60ddcc,
//! game/mission/game.cpp:393-398 and game/vehicles/C_car.cpp.
//!
//! This is an ordered caller contract, not another table owner or a complete
//! reconstruction of C_game::Init. The material helper composes the established
//! definition/projection/replacement owners. The tyre helper borrows the same
//! live car-table resource owner; nonempty particle construction remains an
//! external required operation. No function or
//! runtime-parity claim follows from this module.
#![forbid(unsafe_code)]

use crate::{
    driving_vehicle_tables::{materials, MATERIAL_SOURCE_STRIDE, VEHICLE_MATERIAL_STRIDE},
    game_definition_tables::{DefinitionKind, GameDefinitionTables},
    vehicle_engine_state::{
        vehicle_set_materials, vehicle_set_particles, VehicleMaterialEntry, VehicleMaterialTable,
        VehicleParticleEntry,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarTablesOperation {
    ClearActiveNewMission,
    ReadInternLoad,
    LoadTyres,
    LoadMaterials,
    LoadParticles,
}

/// These are the loaders' native bool results, not C_game::Init's result.
/// The native caller ignores each of them, including false from LoadTyres.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarTablesOutcome {
    Skipped,
    Loaded {
        tyres: bool,
        materials: bool,
        particles: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameInitCarTablesState {
    Ready,
    At(CarTablesOperation),
    Interrupted(CarTablesOperation),
    Complete(CarTablesOutcome),
}

#[derive(Debug, PartialEq, Eq)]
pub enum GameInitCarTablesError<E> {
    Owner {
        operation: CarTablesOperation,
        error: E,
    },
    /// A reached effect, including one whose failure was uncertain, is never
    /// automatically replayed. The surrounding init/lifecycle owns recovery.
    AlreadyStarted,
}

/// Each method must borrow the actual owner when called. In particular, do not
/// preload definitions before LoadTyres or cache intern_load before the clear.
/// No method has a default implementation: an unavailable owner returns Err.
/// Ok(false) means the real native loader completed with false; it is never an
/// encoding for an unimplemented, skipped or failed host operation.
pub trait GameInitCarTablesHost {
    type Error;

    /// PC Init stores zero to the ActiveNewMission byte, even when intern_load
    /// will be false. Any reached mutation must survive a returned host error.
    fn clear_active_new_mission(&mut self) -> Result<(), Self::Error>;
    /// Read the current intern_load byte as zero/nonzero at this exact point.
    fn intern_load(&mut self) -> Result<bool, Self::Error>;
    /// Must include the initial SetTyres(empty), before opening tyres.bin.
    /// Missing/unsupported file version is native false after that replacement.
    fn load_tyres(&mut self) -> Result<bool, Self::Error>;
    fn load_materials(&mut self) -> Result<bool, Self::Error>;
    /// Includes frame/cache operations and old-emitter unlink/release before
    /// replacement. A data-only vector copy is insufficient when emitters exist.
    fn load_particles(&mut self) -> Result<bool, Self::Error>;
}

/// A single invocation's reached boundary and return receipts. It stores no
/// game flags, definition buffers, vehicle tables or native-memory map.
#[derive(Debug)]
pub struct GameInitCarTables {
    state: GameInitCarTablesState,
    loader_returns: [Option<bool>; 3],
}

impl Default for GameInitCarTables {
    fn default() -> Self {
        Self {
            state: GameInitCarTablesState::Ready,
            loader_returns: [None; 3],
        }
    }
}

impl GameInitCarTables {
    pub fn state(&self) -> GameInitCarTablesState {
        self.state
    }

    /// Tyres, materials and particles, in native call order. None means the
    /// loader has not returned; it can already have made partial live changes.
    pub fn loader_returns(&self) -> [Option<bool>; 3] {
        self.loader_returns
    }

    fn effect<T, E>(
        &mut self,
        operation: CarTablesOperation,
        call: impl FnOnce() -> Result<T, E>,
    ) -> Result<T, GameInitCarTablesError<E>> {
        self.state = GameInitCarTablesState::At(operation);
        call().map_err(|error| {
            self.state = GameInitCarTablesState::Interrupted(operation);
            GameInitCarTablesError::Owner { operation, error }
        })
    }

    /// Enter only after the real SegmentsLoad span has returned. Completion
    /// authorizes the next Init operation, DelAllHydrants; it runs none of it.
    pub fn run<H: GameInitCarTablesHost>(
        &mut self,
        host: &mut H,
    ) -> Result<CarTablesOutcome, GameInitCarTablesError<H::Error>> {
        if self.state != GameInitCarTablesState::Ready {
            return Err(GameInitCarTablesError::AlreadyStarted);
        }
        self.effect(CarTablesOperation::ClearActiveNewMission, || {
            host.clear_active_new_mission()
        })?;
        let intern_load = self.effect(CarTablesOperation::ReadInternLoad, || host.intern_load())?;
        let outcome = if intern_load {
            let tyres = self.effect(CarTablesOperation::LoadTyres, || host.load_tyres())?;
            self.loader_returns[0] = Some(tyres);
            let materials =
                self.effect(CarTablesOperation::LoadMaterials, || host.load_materials())?;
            self.loader_returns[1] = Some(materials);
            let particles =
                self.effect(CarTablesOperation::LoadParticles, || host.load_particles())?;
            self.loader_returns[2] = Some(particles);
            CarTablesOutcome::Loaded {
                tyres,
                materials,
                particles,
            }
        } else {
            CarTablesOutcome::Skipped
        };
        self.state = GameInitCarTablesState::Complete(outcome);
        Ok(outcome)
    }
}

/// Run the real tyre lifecycle on the canonical mission-car table owner.
/// This helper creates no table/projection and supplies no successful fallback.
/// The caller retains the file adapter after Err or a native unclosed false exit.
pub(crate) fn load_tyres_from_file<
    B: crate::vehicle_init::VehicleInitBackend,
    F: crate::driving_vehicle_tables::TyreTableFile,
>(
    destination: &mut crate::car_actor_tables::VehicleTableResources,
    host: &mut B,
    file: &mut F,
) -> Result<bool, crate::car_actor_tables::LoadTyresError<B::Error, F::Error>> {
    destination.load_tyres(host, file)
}

/// Availability boundaries of the bounded existing-owner composition. None is
/// a native false return. The original has neither these bounds nor host Err.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CarTableOwnerError {
    MaterialCountOutsideOwnerDomain { count: usize },
    MissingMaterialSource { count: usize },
    ShortMaterialSource { required: usize, available: usize },
    MaterialProjection(String),
    ParticleDefinitionIntakeInterrupted,
    ParticleEmitterCreationUnavailable { definition_count: usize },
    ParticleEmitterCleanupUnavailable { entry_index: usize },
}

/// Compose C_car::LoadMaterials' established projection (RVA 0x1c7f0) with
/// C_Vehicle::SetVehicleMaterials' existing actual table owner (RVA 0xc5c10).
/// Borrow definitions when this call is reached, after LoadTyres has returned.
/// No file is opened here: the live definition owner supplied materialList.
/// Transient records are projected file data, never a second live table.
pub fn load_materials_from_definitions(
    definitions: &GameDefinitionTables,
    destination: &mut VehicleMaterialTable,
) -> Result<bool, CarTableOwnerError> {
    let count = definitions.count(DefinitionKind::Materials);
    // The existing projection and u8-indexed destination cover at most 256
    // rows. Do not truncate a larger native id into an invented valid index.
    if count > 256 {
        return Err(CarTableOwnerError::MaterialCountOutsideOwnerDomain { count });
    }
    let required = count * MATERIAL_SOURCE_STRIDE;
    let source = if count == 0 {
        // Native numOfMaterials < 1 does not dereference materialList. The
        // established projection owns the evidenced default material branch.
        &[][..]
    } else {
        let source = definitions
            .table(DefinitionKind::Materials)
            .ok_or(CarTableOwnerError::MissingMaterialSource { count })?;
        source
            .get(..required)
            .ok_or(CarTableOwnerError::ShortMaterialSource {
                required,
                available: source.len(),
            })?
    };
    // The native count ignores a definition file's incomplete trailing row.
    // Keep the live count authoritative instead of passing the whole file.
    let (rows, _) = materials(source).map_err(CarTableOwnerError::MaterialProjection)?;
    let entries: Vec<_> = rows
        .as_chunks::<VEHICLE_MATERIAL_STRIDE>()
        .0
        .iter()
        .map(|row| VehicleMaterialEntry {
            // Projection IDs are 0..count (or the single default 0).
            id: row[0],
            payload: std::array::from_fn(|index| {
                let at = 4 + index * 4;
                u32::from_le_bytes(row[at..at + 4].try_into().unwrap())
            }),
        })
        .collect();
    // Use the real setter, including its preservation of conversion slot ff
    // when no projected row writes that slot. Do not publish projection's map.
    Ok(vehicle_set_materials(destination, &entries))
}

pub(crate) fn require_emitter_free_definitions(
    definitions: &GameDefinitionTables,
) -> Result<(), CarTableOwnerError> {
    if definitions.interrupted() || definitions.pending_file().is_some() {
        return Err(CarTableOwnerError::ParticleDefinitionIntakeInterrupted);
    }
    let definition_count = definitions.count(DefinitionKind::Particles);
    if definition_count != 0 {
        return Err(CarTableOwnerError::ParticleEmitterCreationUnavailable { definition_count });
    }
    Ok(())
}

/// Bounded LoadParticles composition (RVA 0x1ca80) for the evidenced zero-count
/// branch only. Nonempty definitions need real driver/model-cache/emitter
/// owners. A prior nonnull emitter also needs CloseParticles (RVA 0xc5f80),
/// which the existing data-copy setter deliberately does not implement.
pub fn load_particles_without_emitters(
    definitions: &GameDefinitionTables,
    destination: &mut Vec<VehicleParticleEntry>,
) -> Result<bool, CarTableOwnerError> {
    require_emitter_free_definitions(definitions)?;
    // CloseParticles visits the old entries backwards. Reaching an emitter is
    // an unsupported unlink boundary, not permission to clear its ownership.
    for (entry_index, entry) in destination.iter().enumerate().rev() {
        if entry.emitter != 0 {
            return Err(CarTableOwnerError::ParticleEmitterCleanupUnavailable { entry_index });
        }
    }
    Ok(vehicle_set_particles(
        destination,
        &[VehicleParticleEntry::default()],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_definition_tables::{DefinitionTableBackend, TableFile};

    const ORDER: [CarTablesOperation; 5] = [
        CarTablesOperation::ClearActiveNewMission,
        CarTablesOperation::ReadInternLoad,
        CarTablesOperation::LoadTyres,
        CarTablesOperation::LoadMaterials,
        CarTablesOperation::LoadParticles,
    ];

    #[derive(Debug, PartialEq, Eq)]
    enum TestError {
        Interrupted,
        Owner(CarTableOwnerError),
    }

    // Scripted callbacks check caller semantics only. They are not substitutes
    // for native loaders in production or original-runtime validation.
    struct Host {
        active_new_mission: u8,
        intern_load: u8,
        gate_after_clear: Option<u8>,
        events: Vec<CarTablesOperation>,
        fail_after: Option<CarTablesOperation>,
        results: [bool; 3],
        definitions_after_tyres: Option<GameDefinitionTables>,
        definitions: GameDefinitionTables,
        materials: VehicleMaterialTable,
        particles: Vec<VehicleParticleEntry>,
        compose_existing: bool,
    }

    impl Default for Host {
        fn default() -> Self {
            Self {
                active_new_mission: 0x80,
                intern_load: 1,
                gate_after_clear: None,
                events: Vec::new(),
                fail_after: None,
                results: [true; 3],
                definitions_after_tyres: None,
                definitions: GameDefinitionTables::default(),
                materials: VehicleMaterialTable::default(),
                particles: Vec::new(),
                compose_existing: false,
            }
        }
    }

    impl Host {
        fn returned<T>(
            &mut self,
            operation: CarTablesOperation,
            result: T,
        ) -> Result<T, TestError> {
            if self.fail_after == Some(operation) {
                Err(TestError::Interrupted)
            } else {
                Ok(result)
            }
        }
    }

    impl GameInitCarTablesHost for Host {
        type Error = TestError;
        fn clear_active_new_mission(&mut self) -> Result<(), Self::Error> {
            self.events.push(CarTablesOperation::ClearActiveNewMission);
            self.active_new_mission = 0;
            if let Some(gate) = self.gate_after_clear {
                self.intern_load = gate;
            }
            self.returned(CarTablesOperation::ClearActiveNewMission, ())
        }
        fn intern_load(&mut self) -> Result<bool, Self::Error> {
            self.events.push(CarTablesOperation::ReadInternLoad);
            self.returned(CarTablesOperation::ReadInternLoad, self.intern_load != 0)
        }
        fn load_tyres(&mut self) -> Result<bool, Self::Error> {
            self.events.push(CarTablesOperation::LoadTyres);
            if let Some(definitions) = self.definitions_after_tyres.take() {
                self.definitions = definitions;
            }
            self.returned(CarTablesOperation::LoadTyres, self.results[0])
        }
        fn load_materials(&mut self) -> Result<bool, Self::Error> {
            self.events.push(CarTablesOperation::LoadMaterials);
            let result = if self.compose_existing {
                load_materials_from_definitions(&self.definitions, &mut self.materials)
                    .map_err(TestError::Owner)?
            } else {
                self.results[1]
            };
            self.returned(CarTablesOperation::LoadMaterials, result)
        }
        fn load_particles(&mut self) -> Result<bool, Self::Error> {
            self.events.push(CarTablesOperation::LoadParticles);
            let result = if self.compose_existing {
                load_particles_without_emitters(&self.definitions, &mut self.particles)
                    .map_err(TestError::Owner)?
            } else {
                self.results[2]
            };
            self.returned(CarTablesOperation::LoadParticles, result)
        }
    }

    #[derive(Default)]
    struct DefinitionFiles {
        material: Vec<u8>,
        particle: Vec<u8>,
        failure: Option<DefinitionReloadFailure>,
    }

    #[derive(Clone, Copy, PartialEq)]
    enum DefinitionReloadFailure {
        SeekMaterials,
        ReadShortMaterials,
    }
    impl DefinitionTableBackend for DefinitionFiles {
        type Error = ();
        fn open(&mut self, kind: DefinitionKind) -> Result<Option<TableFile>, ()> {
            Ok(Some(TableFile(match kind {
                DefinitionKind::Items => 0,
                DefinitionKind::Materials => 1,
                DefinitionKind::Particles => 2,
                DefinitionKind::Voices => 3,
            })))
        }
        fn seek_end(&mut self, file: TableFile) -> Result<usize, ()> {
            if file.0 == 1 && self.failure == Some(DefinitionReloadFailure::SeekMaterials) {
                return Err(());
            }
            Ok(match file.0 {
                1 => self.material.len(),
                2 => self.particle.len(),
                _ => 0,
            })
        }
        fn seek_begin(&mut self, _: TableFile) -> Result<(), ()> {
            Ok(())
        }
        fn allocate(&mut self, bytes: usize) -> Result<Vec<u8>, ()> {
            let available = if self.failure == Some(DefinitionReloadFailure::ReadShortMaterials) {
                bytes.saturating_sub(1)
            } else {
                bytes
            };
            Ok(vec![0; available])
        }
        fn read_into(&mut self, file: TableFile, bytes: &mut [u8]) -> Result<(), ()> {
            if file.0 == 1 && self.failure == Some(DefinitionReloadFailure::ReadShortMaterials) {
                return Err(());
            }
            match file.0 {
                1 => bytes.copy_from_slice(&self.material),
                2 => bytes.copy_from_slice(&self.particle),
                _ => {}
            }
            Ok(())
        }
        fn close(&mut self, _: TableFile) -> Result<(), ()> {
            Ok(())
        }
    }

    fn definitions(material: Vec<u8>, particle: Vec<u8>) -> GameDefinitionTables {
        let mut result = GameDefinitionTables::default();
        assert_eq!(
            result.load(&mut DefinitionFiles {
                material,
                particle,
                ..DefinitionFiles::default()
            }),
            Ok(true)
        );
        result
    }

    #[test]
    fn clear_precedes_live_gate_and_gate_false_completes_without_loaders() {
        let mut host = Host {
            gate_after_clear: Some(0),
            ..Host::default()
        };
        let mut span = GameInitCarTables::default();
        assert_eq!(span.run(&mut host), Ok(CarTablesOutcome::Skipped));
        assert_eq!(host.active_new_mission, 0);
        assert_eq!(host.events, ORDER[..2]);
        assert_eq!(span.loader_returns(), [None; 3]);
        assert_eq!(
            span.state(),
            GameInitCarTablesState::Complete(CarTablesOutcome::Skipped)
        );
    }

    #[test]
    fn live_nonzero_gate_and_native_false_keep_all_three_calls_in_order() {
        let mut host = Host {
            intern_load: 0,
            gate_after_clear: Some(0x80),
            results: [false; 3],
            ..Host::default()
        };
        let mut span = GameInitCarTables::default();
        assert_eq!(
            span.run(&mut host),
            Ok(CarTablesOutcome::Loaded {
                tyres: false,
                materials: false,
                particles: false,
            })
        );
        assert_eq!(host.events, ORDER);
        assert_eq!(span.loader_returns(), [Some(false); 3]);
        assert_eq!(
            span.run(&mut host),
            Err(GameInitCarTablesError::AlreadyStarted)
        );
        assert_eq!(host.events, ORDER);
    }

    #[test]
    fn each_interrupted_boundary_keeps_reached_prefix_and_cannot_replay() {
        for (index, operation) in ORDER.into_iter().enumerate() {
            let mut host = Host {
                fail_after: Some(operation),
                ..Host::default()
            };
            let mut span = GameInitCarTables::default();
            assert_eq!(
                span.run(&mut host),
                Err(GameInitCarTablesError::Owner {
                    operation,
                    error: TestError::Interrupted,
                })
            );
            assert_eq!(host.active_new_mission, 0);
            assert_eq!(host.events, ORDER[..=index]);
            assert_eq!(span.state(), GameInitCarTablesState::Interrupted(operation));
            let completed = index.saturating_sub(2);
            for (receipt, expected_returned) in span
                .loader_returns()
                .into_iter()
                .zip((0..3).map(|loader| loader < completed))
            {
                assert_eq!(receipt, expected_returned.then_some(true));
            }
            assert_eq!(
                span.run(&mut host),
                Err(GameInitCarTablesError::AlreadyStarted)
            );
            assert_eq!(host.events, ORDER[..=index]);
        }
    }

    #[test]
    fn material_helper_observes_definition_change_made_by_tyres_callback() {
        let mut material = vec![0; MATERIAL_SOURCE_STRIDE];
        let nan_bits = 0x7fc1_2345_u32;
        material[0x108..0x10c].copy_from_slice(&nan_bits.to_le_bytes());
        let mut host = Host {
            definitions_after_tyres: Some(definitions(material, Vec::new())),
            compose_existing: true,
            ..Host::default()
        };
        host.materials.conv[255] = 0xa7;
        let mut span = GameInitCarTables::default();
        assert!(span.run(&mut host).is_ok());
        assert_eq!(host.materials.entries.len(), 1);
        assert_eq!(host.materials.entries[0].payload[5], nan_bits);
        assert_eq!(host.materials.conv[255], 0xa7);
        assert_eq!(host.particles, [VehicleParticleEntry::default()]);
        assert_eq!(host.events, ORDER);
    }

    #[test]
    fn unsupported_particle_creation_retains_completed_material_replacement() {
        let old = VehicleMaterialEntry {
            id: 7,
            payload: [0xfeed; 11],
        };
        let mut host = Host {
            definitions: definitions(Vec::new(), vec![0; 0xa4]),
            materials: VehicleMaterialTable {
                entries: vec![old],
                conv: [0x71; 256],
            },
            compose_existing: true,
            ..Host::default()
        };
        let mut span = GameInitCarTables::default();
        assert_eq!(
            span.run(&mut host),
            Err(GameInitCarTablesError::Owner {
                operation: CarTablesOperation::LoadParticles,
                error: TestError::Owner(CarTableOwnerError::ParticleEmitterCreationUnavailable {
                    definition_count: 1,
                }),
            })
        );
        assert_eq!(span.loader_returns(), [Some(true), Some(true), None]);
        assert_eq!(host.materials.entries.len(), 1);
        assert_eq!(host.materials.entries[0].id, 0);
        assert_eq!(host.materials.entries[0].payload[0], 100.0_f32.to_bits());
        assert_eq!(host.materials.conv[255], 0x71);
        assert!(host.particles.is_empty());
    }

    #[test]
    fn material_file_tail_is_ignored_using_the_actual_definition_count() {
        let source = definitions(vec![0; MATERIAL_SOURCE_STRIDE + 13], Vec::new());
        let mut destination = VehicleMaterialTable::default();
        assert_eq!(
            load_materials_from_definitions(&source, &mut destination),
            Ok(true)
        );
        assert_eq!(destination.entries.len(), 1);
        let source = definitions(vec![0; 256 * MATERIAL_SOURCE_STRIDE], Vec::new());
        destination.conv[255] = 0x44;
        assert_eq!(
            load_materials_from_definitions(&source, &mut destination),
            Ok(true)
        );
        assert_eq!(destination.entries.len(), 256);
        assert_eq!(destination.conv[255], 255);
    }

    #[test]
    fn unsupported_material_extent_keeps_the_destination_owner_unchanged() {
        let source = definitions(vec![0; 257 * MATERIAL_SOURCE_STRIDE], Vec::new());
        let mut destination = VehicleMaterialTable {
            entries: vec![VehicleMaterialEntry {
                id: 9,
                payload: [8; 11],
            }],
            conv: [7; 256],
        };
        let before = destination.clone();
        assert_eq!(
            load_materials_from_definitions(&source, &mut destination),
            Err(CarTableOwnerError::MaterialCountOutsideOwnerDomain { count: 257 })
        );
        assert_eq!(destination, before);
    }

    #[test]
    fn interrupted_definition_reload_does_not_publish_default_materials() {
        use crate::game_definition_tables::TableLoadError;

        for failure in [
            DefinitionReloadFailure::SeekMaterials,
            DefinitionReloadFailure::ReadShortMaterials,
        ] {
            let mut source = definitions(vec![0; MATERIAL_SOURCE_STRIDE], Vec::new());
            let mut reload = DefinitionFiles {
                material: vec![0; MATERIAL_SOURCE_STRIDE],
                failure: Some(failure),
                ..DefinitionFiles::default()
            };
            assert_eq!(source.load(&mut reload), Err(TableLoadError::Backend(())));
            assert_eq!(source.count(DefinitionKind::Materials), 1);
            assert!(source.interrupted());
            let expected = match failure {
                DefinitionReloadFailure::SeekMaterials => {
                    CarTableOwnerError::MissingMaterialSource { count: 1 }
                }
                DefinitionReloadFailure::ReadShortMaterials => {
                    CarTableOwnerError::ShortMaterialSource {
                        required: MATERIAL_SOURCE_STRIDE,
                        available: MATERIAL_SOURCE_STRIDE - 1,
                    }
                }
            };
            let mut destination = VehicleMaterialTable {
                entries: vec![VehicleMaterialEntry {
                    id: 9,
                    payload: [8; 11],
                }],
                conv: [7; 256],
            };
            let before = destination.clone();
            assert_eq!(
                load_materials_from_definitions(&source, &mut destination),
                Err(expected)
            );
            assert_eq!(destination, before);
        }
    }

    #[test]
    fn particle_cleanup_stops_at_last_live_emitter_without_clearing_ownership() {
        let source = GameDefinitionTables::default();
        let mut destination = vec![
            VehicleParticleEntry {
                emitter: 3,
                ..VehicleParticleEntry::default()
            },
            VehicleParticleEntry {
                emitter: 8,
                ..VehicleParticleEntry::default()
            },
            VehicleParticleEntry::default(),
        ];
        let before = destination.clone();
        assert_eq!(
            load_particles_without_emitters(&source, &mut destination),
            Err(CarTableOwnerError::ParticleEmitterCleanupUnavailable { entry_index: 1 })
        );
        assert_eq!(destination, before);
        destination.iter_mut().for_each(|entry| entry.emitter = 0);
        assert_eq!(
            load_particles_without_emitters(&source, &mut destination),
            Ok(true)
        );
        assert_eq!(destination, [VehicleParticleEntry::default()]);
    }
}
