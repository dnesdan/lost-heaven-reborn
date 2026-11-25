use super::{ActorWorld, SourceBacking};
use crate::{
    driving_vehicle_tables::{MATERIAL_SOURCE_STRIDE, VEHICLE_MATERIAL_STRIDE},
    game_definition_tables::{
        DefinitionKind, DefinitionTableBackend, GameDefinitionTables, TableFile,
    },
    model_loading::ModelArena,
    vehicle_frame_backend::{VehicleFrameBackend, VehicleFrameMemory},
    vehicle_init::VehicleInitBackend,
};
use lhr_assets::Installation;

const MATERIAL_INDEX: u32 = 0x63d8b0;
const VEHICLE_MATERIALS: u32 = 0x63d9b4;
const TYRE_BEGIN: u32 = 0x63d9c8;
const SOURCE_MATERIALS: u32 = 0x6d4c24;
const SOURCE_COUNT: u32 = 0x6d4c28;

struct DefinitionFiles<'a> {
    materials: Option<&'a [u8]>,
    fail_material_close: bool,
}

impl DefinitionFiles<'_> {
    fn bytes(&self, file: TableFile) -> &[u8] {
        match file.0 {
            1 => self.materials.unwrap(),
            _ => &[],
        }
    }
}

impl DefinitionTableBackend for DefinitionFiles<'_> {
    type Error = &'static str;

    fn open(&mut self, kind: DefinitionKind) -> Result<Option<TableFile>, Self::Error> {
        Ok(match kind {
            DefinitionKind::Materials => self.materials.map(|_| TableFile(1)),
            _ => Some(TableFile(0)),
        })
    }
    fn seek_end(&mut self, file: TableFile) -> Result<usize, Self::Error> {
        Ok(self.bytes(file).len())
    }
    fn seek_begin(&mut self, _: TableFile) -> Result<(), Self::Error> {
        Ok(())
    }
    fn allocate(&mut self, size: usize) -> Result<Vec<u8>, Self::Error> {
        Ok(vec![0; size])
    }
    fn read_into(&mut self, file: TableFile, bytes: &mut [u8]) -> Result<(), Self::Error> {
        bytes.copy_from_slice(self.bytes(file));
        Ok(())
    }
    fn close(&mut self, file: TableFile) -> Result<(), Self::Error> {
        if file == TableFile(1) && self.fail_material_close {
            return Err("synthetic material close failure");
        }
        Ok(())
    }
}

fn startup_definitions(materials: Option<&[u8]>) -> GameDefinitionTables {
    let mut tables = GameDefinitionTables::default();
    assert_eq!(
        tables.load(&mut DefinitionFiles {
            materials,
            fail_material_close: false,
        }),
        Ok(materials.is_some())
    );
    tables
}

fn configured_world() -> ActorWorld {
    let empty_source = SourceBacking {
        storage: None,
        capacity: 0,
    };
    let mut world = ActorWorld::new(0, 0, empty_source, empty_source, empty_source).unwrap();
    world.configure_car_resources(1).unwrap();
    world
}

fn with_car_backend<R>(
    world: &mut ActorWorld,
    arena: &mut ModelArena,
    installation: Option<&Installation>,
    read: impl FnOnce(&mut VehicleFrameBackend<'_, VehicleFrameMemory>) -> R,
) -> R {
    world
        .car_services
        .as_mut()
        .unwrap()
        .with_precision(|services| read(&mut services.init_backend(arena, installation)))
}

fn material_rows(count: usize, first_value: u32) -> Vec<u8> {
    let mut bytes = vec![0; count * MATERIAL_SOURCE_STRIDE];
    for (index, row) in bytes
        .as_chunks_mut::<MATERIAL_SOURCE_STRIDE>()
        .0
        .iter_mut()
        .enumerate()
    {
        let value = first_value + index as u32;
        row[..4].copy_from_slice(&(!value).to_le_bytes());
        row[0x124..0x128].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

struct MaterialInstallation(std::path::PathBuf);

impl MaterialInstallation {
    fn new(bytes: &[u8]) -> Self {
        let root = std::env::temp_dir().join(format!(
            "lhr-car-material-publication-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir(&root).unwrap();
        let fixture = Self(root);
        std::fs::create_dir(fixture.0.join("tables")).unwrap();
        fixture.write(bytes);
        fixture
    }
    fn write(&self, bytes: &[u8]) {
        std::fs::write(self.0.join("tables/materials.def"), bytes).unwrap();
    }
}

impl Drop for MaterialInstallation {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn car_backend_reads_live_materials_across_lazy_and_startup_replacements() {
    let fixture = MaterialInstallation::new(&material_rows(1, 10));
    let installation = Installation::open(&fixture.0).unwrap();
    let mut arena = ModelArena::default();
    let mut world = configured_world();

    let (lazy_rows, lazy_source) =
        with_car_backend(&mut world, &mut arena, Some(&installation), |car| {
            let rows = car.read_word(VEHICLE_MATERIALS).unwrap();
            let source = car.read_word(SOURCE_MATERIALS).unwrap();
            assert_eq!(car.read_word(SOURCE_COUNT).unwrap(), 1);
            assert_eq!(car.read_word(rows + 0x20).unwrap(), 10);
            assert_eq!(car.read_word(source).unwrap(), !10u32);
            assert_eq!(car.read_byte(MATERIAL_INDEX + 0xff).unwrap(), 0);
            (rows, source)
        });
    assert_eq!(world.live_car_table_allocation_count(), 2);
    fixture.write(&[0xff]); // A later installation read would fail projection.

    let mut startup_rows = material_rows(256, 100);
    startup_rows.push(0xa5);
    world
        .load_car_materials_for_init(&startup_definitions(Some(&startup_rows)))
        .unwrap();
    let (full_rows, full_source) =
        with_car_backend(&mut world, &mut arena, Some(&installation), |car| {
            let rows = car.read_word(VEHICLE_MATERIALS).unwrap();
            let source = car.read_word(SOURCE_MATERIALS).unwrap();
            assert_eq!(car.read_word(SOURCE_COUNT).unwrap(), 256);
            assert_eq!(car.read_byte(MATERIAL_INDEX + 0xff).unwrap(), 0xff);
            assert_eq!(
                car.read_byte(source + 256 * MATERIAL_SOURCE_STRIDE as u32)
                    .unwrap(),
                0xa5
            );
            for id in 0..256 {
                let row = rows + id * VEHICLE_MATERIAL_STRIDE as u32;
                assert_eq!(car.read_word(row).unwrap(), id);
                assert_eq!(car.read_word(row + 0x20).unwrap(), 100 + id);
            }
            assert!(car.read_byte(lazy_rows).is_err());
            assert!(car.read_byte(lazy_source).is_err());
            (rows, source)
        });
    assert_eq!(world.live_car_table_allocation_count(), 2);

    world
        .load_car_materials_for_init(&startup_definitions(Some(&material_rows(2, 500))))
        .unwrap();
    with_car_backend(&mut world, &mut arena, None, |car| {
        let rows = car.read_word(VEHICLE_MATERIALS).unwrap();
        let source = car.read_word(SOURCE_MATERIALS).unwrap();
        assert_eq!(car.read_word(SOURCE_COUNT).unwrap(), 2);
        assert_eq!(
            car.read_word(rows + VEHICLE_MATERIAL_STRIDE as u32 + 0x20)
                .unwrap(),
            501
        );
        assert_eq!(
            car.read_word(source + MATERIAL_SOURCE_STRIDE as u32)
                .unwrap(),
            !501u32
        );
        assert_eq!(car.read_byte(MATERIAL_INDEX + 1).unwrap(), 1);
        assert_eq!(car.read_byte(MATERIAL_INDEX + 2).unwrap(), 0);
        assert_eq!(car.read_byte(MATERIAL_INDEX + 0xff).unwrap(), 0xff);
        assert!(car.read_byte(full_rows).is_err());
        assert!(car.read_byte(full_source).is_err());
    });
    assert_eq!(world.live_car_table_allocation_count(), 2);
    world.discard_car_services_host().unwrap();
    assert_eq!(world.live_car_table_allocation_count(), 0);
}

#[test]
fn reached_zero_count_uses_native_default_independent_of_interrupted_tyres() {
    let mut arena = ModelArena::default();
    let mut world = configured_world();
    with_car_backend(&mut world, &mut arena, None, |car| {
        assert!(car.read_word(TYRE_BEGIN).is_err());
    });

    for (source, expected_allocations) in [(None, 1), (Some(&[0xa5][..]), 2)] {
        world
            .load_car_materials_for_init(&startup_definitions(source))
            .unwrap();
        with_car_backend(&mut world, &mut arena, None, |car| {
            assert!(car.read_word(TYRE_BEGIN).is_err());
            assert_eq!(car.read_word(SOURCE_COUNT).unwrap(), 0);
            let raw = car.read_word(SOURCE_MATERIALS).unwrap();
            if let Some(source) = source {
                assert_eq!(car.read_byte(raw).unwrap(), source[0]);
            } else {
                assert_eq!(raw, 0);
            }
            let rows = car.read_word(VEHICLE_MATERIALS).unwrap();
            assert_eq!(car.read_word(rows).unwrap(), 0);
            assert_eq!(car.read_word(rows + 0x04).unwrap(), 100f32.to_bits());
            assert_eq!(car.read_word(rows + 0x18).unwrap(), 0.35f32.to_bits());
        });
        assert_eq!(
            world.live_car_table_allocation_count(),
            expected_allocations
        );
    }
}

#[test]
fn incomplete_startup_definitions_latch_publication_even_if_the_old_count_survives() {
    let old_source = material_rows(1, 10);
    let mut definitions = startup_definitions(Some(&old_source));
    let mut world = configured_world();
    world.load_car_materials_for_init(&definitions).unwrap();

    let new_source = material_rows(2, 20);
    assert!(definitions
        .load(&mut DefinitionFiles {
            materials: Some(&new_source),
            fail_material_close: true,
        })
        .is_err());
    assert_eq!(definitions.count(DefinitionKind::Materials), 1);
    assert_eq!(
        definitions.table(DefinitionKind::Materials),
        Some(new_source.as_slice())
    );

    let error = world.load_car_materials_for_init(&definitions).unwrap_err();
    assert!(error.contains("Startup material definitions interrupted"));
    assert!(world
        .load_car_materials_for_init(&startup_definitions(Some(&old_source)))
        .is_err());
    with_car_backend(&mut world, &mut ModelArena::default(), None, |car| {
        assert!(car.read_word(VEHICLE_MATERIALS).is_err());
    });
    assert_eq!(world.live_car_table_allocation_count(), 2);
    world.discard_car_services_host().unwrap();
    assert_eq!(world.live_car_table_allocation_count(), 0);
}

#[test]
fn startup_count_above_the_byte_index_domain_is_rejected_without_truncation() {
    let mut world = configured_world();
    let source = material_rows(257, 10);
    let error = world
        .load_car_materials_for_init(&startup_definitions(Some(&source)))
        .unwrap_err();
    assert!(error.contains("MaterialCountOutsideOwnerDomain { count: 257 }"));
    assert_eq!(world.live_car_table_allocation_count(), 0);
}
