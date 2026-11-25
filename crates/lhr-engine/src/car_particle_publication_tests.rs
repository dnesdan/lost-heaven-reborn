use super::{ActorWorld, SourceBacking};
use crate::{
    game_definition_tables::{
        DefinitionKind, DefinitionTableBackend, GameDefinitionTables, TableFile,
    },
    model_loading::ModelArena,
    vehicle_frame_backend::{VehicleFrameBackend, VehicleFrameMemory},
    vehicle_init::VehicleInitBackend,
};
use lhr_assets::Installation;

const PARTICLE_BEGIN: u32 = 0x63d8a4;
const PARTICLE_END: u32 = 0x63d8a8;
const RECORD_BYTES: u32 = 0x1c;

struct DefinitionFiles {
    particle_bytes: usize,
    fail_particle_close: bool,
}

impl DefinitionTableBackend for DefinitionFiles {
    type Error = &'static str;

    fn open(&mut self, kind: DefinitionKind) -> Result<Option<TableFile>, Self::Error> {
        Ok(Some(TableFile(u64::from(
            kind == DefinitionKind::Particles,
        ))))
    }
    fn seek_end(&mut self, file: TableFile) -> Result<usize, Self::Error> {
        Ok(if file == TableFile(1) {
            self.particle_bytes
        } else {
            0
        })
    }
    fn seek_begin(&mut self, _: TableFile) -> Result<(), Self::Error> {
        Ok(())
    }
    fn allocate(&mut self, bytes: usize) -> Result<Vec<u8>, Self::Error> {
        Ok(vec![0; bytes])
    }
    fn read_into(&mut self, _: TableFile, bytes: &mut [u8]) -> Result<(), Self::Error> {
        bytes.fill(0xa5);
        Ok(())
    }
    fn close(&mut self, file: TableFile) -> Result<(), Self::Error> {
        if file == TableFile(1) && self.fail_particle_close {
            return Err("synthetic particle close failure");
        }
        Ok(())
    }
}

fn definitions(particle_bytes: usize) -> GameDefinitionTables {
    let mut tables = GameDefinitionTables::default();
    assert_eq!(
        tables.load(&mut DefinitionFiles {
            particle_bytes,
            fail_particle_close: false,
        }),
        Ok(true)
    );
    tables
}

fn configured_world() -> ActorWorld {
    let empty = SourceBacking {
        storage: None,
        capacity: 0,
    };
    let mut world = ActorWorld::new(0, 0, empty, empty, empty).unwrap();
    world.configure_car_resources(1).unwrap();
    world
}

fn with_car_backend<R>(
    world: &mut ActorWorld,
    arena: &mut ModelArena,
    installation: Option<&Installation>,
    operation: impl FnOnce(&mut VehicleFrameBackend<'_, VehicleFrameMemory>) -> R,
) -> R {
    world
        .car_services
        .as_mut()
        .unwrap()
        .with_precision(|services| operation(&mut services.init_backend(arena, installation)))
}

fn assert_zero_record(car: &mut VehicleFrameBackend<'_, VehicleFrameMemory>) -> u32 {
    let begin = car.read_word(PARTICLE_BEGIN).unwrap();
    assert_ne!(begin, 0);
    assert_eq!(car.read_word(PARTICLE_END).unwrap(), begin + RECORD_BYTES);
    for offset in 0..RECORD_BYTES {
        assert_eq!(car.read_byte(begin + offset).unwrap(), 0);
    }
    assert!(car.read_byte(begin + RECORD_BYTES).is_err());
    begin
}

struct ParticleInstallation(std::path::PathBuf);

impl ParticleInstallation {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "lhr-car-particle-publication-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir(&root).unwrap();
        let fixture = Self(root);
        std::fs::create_dir(fixture.0.join("tables")).unwrap();
        std::fs::write(
            fixture.0.join(DefinitionKind::Particles.path()),
            vec![0xa5; DefinitionKind::Particles.stride()],
        )
        .unwrap();
        std::fs::write(fixture.0.join("tables/tyres.bin"), [1, 0, 0, 0, 0, 0, 0, 0]).unwrap();
        fixture
    }
}

impl Drop for ParticleInstallation {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn particle_globals_require_init_and_use_the_startup_count() {
    let fixture = ParticleInstallation::new();
    let installation = Installation::open(&fixture.0).unwrap();
    let mut arena = ModelArena::default();
    let mut world = configured_world();
    with_car_backend(&mut world, &mut arena, Some(&installation), |car| {
        for address in PARTICLE_BEGIN..PARTICLE_END + 4 {
            assert!(car.read_byte(address).is_err());
        }
        assert!(car.read_word(PARTICLE_BEGIN).is_err());
        assert!(car.read_word(PARTICLE_END).is_err());
    });
    assert_eq!(world.live_car_table_allocation_count(), 0);

    // An installation file with a full definition is available. The admitted
    // source contains a tail only, so its actual definition count is zero.
    let source = definitions(DefinitionKind::Particles.stride() - 1);
    assert_eq!(world.load_car_particles_for_init(&source), Ok(true));
    with_car_backend(&mut world, &mut arena, Some(&installation), |car| {
        assert_zero_record(car);
    });
    assert_eq!(world.live_car_table_allocation_count(), 1);
    world.discard_car_services_host().unwrap();
    assert_eq!(world.live_car_table_allocation_count(), 0);
}

#[test]
fn particle_replacement_reuses_capacity_and_preserves_tyres_and_materials() {
    const OTHER_TABLES: [u32; 3] = [0x63d9b4, 0x63d9c8, 0x63d9cc];
    let fixture = ParticleInstallation::new();
    let installation = Installation::open(&fixture.0).unwrap();
    let mut arena = ModelArena::default();
    let mut world = configured_world();
    let source = GameDefinitionTables::default();
    assert_eq!(world.load_car_tyres_for_init(&installation), Ok(true));
    world.load_car_materials_for_init(&source).unwrap();
    let before = world.live_car_table_allocation_count();
    assert_eq!(world.load_car_particles_for_init(&source), Ok(true));

    let (begin, other_tables) = with_car_backend(&mut world, &mut arena, None, |car| {
        let begin = assert_zero_record(car);
        for offset in (4..RECORD_BYTES).step_by(4) {
            car.write_word(begin + offset, 0xa5a5_0000 | offset)
                .unwrap();
        }
        // The bounded vector header is controlled by the table owner.
        assert!(car.write_word(PARTICLE_END, u32::MAX).is_err());
        (
            begin,
            OTHER_TABLES.map(|address| car.read_word(address).unwrap()),
        )
    });
    assert_eq!(world.load_car_particles_for_init(&source), Ok(true));
    with_car_backend(&mut world, &mut arena, None, |car| {
        assert_eq!(assert_zero_record(car), begin);
        assert_eq!(
            OTHER_TABLES.map(|address| car.read_word(address).unwrap()),
            other_tables
        );
        assert_eq!(
            car.read_word(other_tables[0] + 4).unwrap(),
            100f32.to_bits()
        );
        assert_eq!(car.read_word(other_tables[1]).unwrap(), 0);
    });
    assert_eq!(world.live_car_table_allocation_count(), before + 1);
    world.discard_car_services_host().unwrap();
    assert_eq!(world.live_car_table_allocation_count(), 0);
}

#[test]
fn nonzero_definition_count_refuses_publication_and_blocks_replay() {
    let mut arena = ModelArena::default();
    let zero = GameDefinitionTables::default();
    let nonzero = definitions(2 * DefinitionKind::Particles.stride() + 1);
    for already_loaded in [false, true] {
        let mut world = configured_world();
        let old = already_loaded.then(|| {
            assert_eq!(world.load_car_particles_for_init(&zero), Ok(true));
            with_car_backend(&mut world, &mut arena, None, |car| {
                let begin = assert_zero_record(car);
                car.write_word(begin + 4, 0xa5a5_5a5a).unwrap();
                begin
            })
        });
        let error = world.load_car_particles_for_init(&nonzero).unwrap_err();
        assert!(error.contains("definition_count: 2"), "{error}");
        assert!(error.contains("emitter creation"), "{error}");
        assert_eq!(
            world.live_car_table_allocation_count(),
            usize::from(already_loaded)
        );
        assert!(world
            .load_car_particles_for_init(&zero)
            .unwrap_err()
            .contains("Interrupted"));
        with_car_backend(&mut world, &mut arena, None, |car| {
            if let Some(begin) = old {
                assert_eq!(car.read_word(PARTICLE_BEGIN).unwrap(), begin);
                assert_eq!(car.read_word(PARTICLE_END).unwrap(), begin + RECORD_BYTES);
                assert_eq!(car.read_word(begin + 4).unwrap(), 0xa5a5_5a5a);
            } else {
                assert!(car.read_word(PARTICLE_BEGIN).is_err());
                assert!(car.read_word(PARTICLE_END).is_err());
            }
        });
        world.discard_car_services_host().unwrap();
    }
}

#[test]
fn old_live_emitter_survives_replacement_and_host_disposal_refusal() {
    const EMITTER: u32 = 0x1234_abcd;
    let mut arena = ModelArena::default();
    let mut world = configured_world();
    let source = GameDefinitionTables::default();
    assert_eq!(world.load_car_particles_for_init(&source), Ok(true));
    let begin = with_car_backend(&mut world, &mut arena, None, |car| {
        let begin = assert_zero_record(car);
        car.write_word(begin, EMITTER).unwrap();
        car.write_word(begin + 4, 0x7654_3210).unwrap();
        begin
    });
    let error = world.load_car_particles_for_init(&source).unwrap_err();
    assert!(error.contains("CloseParticles"), "{error}");
    assert!(error.contains("0x1234abcd"), "{error}");
    assert!(world
        .load_car_particles_for_init(&source)
        .unwrap_err()
        .contains("Interrupted"));
    with_car_backend(&mut world, &mut arena, None, |car| {
        assert_eq!(car.read_word(PARTICLE_BEGIN).unwrap(), begin);
        assert_eq!(car.read_word(PARTICLE_END).unwrap(), begin + RECORD_BYTES);
        assert_eq!(car.read_word(begin).unwrap(), EMITTER);
        assert_eq!(car.read_word(begin + 4).unwrap(), 0x7654_3210);
    });

    assert!(world.discard_car_services_host().is_err());
    assert_eq!(world.live_car_table_allocation_count(), 1);
    with_car_backend(&mut world, &mut arena, None, |car| {
        assert_eq!(car.read_word(begin).unwrap(), EMITTER);
        car.write_word(begin, 0).unwrap();
    });
    assert!(world.discard_car_services_host().is_err());
    assert_eq!(world.live_car_table_allocation_count(), 1);
}

#[test]
fn interrupted_definition_intake_cannot_publish_from_an_old_zero_count() {
    let mut arena = ModelArena::default();
    let mut world = configured_world();
    let mut source = GameDefinitionTables::default();
    assert!(source
        .load(&mut DefinitionFiles {
            particle_bytes: DefinitionKind::Particles.stride(),
            fail_particle_close: true,
        })
        .is_err());
    assert_eq!(source.count(DefinitionKind::Particles), 0);
    assert!(world
        .load_car_particles_for_init(&source)
        .unwrap_err()
        .contains("ParticleDefinitionIntakeInterrupted"));
    assert_eq!(world.live_car_table_allocation_count(), 0);
    with_car_backend(&mut world, &mut arena, None, |car| {
        assert!(car.read_word(PARTICLE_BEGIN).is_err());
    });
}
