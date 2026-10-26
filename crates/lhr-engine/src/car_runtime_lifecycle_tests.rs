use super::*;
use crate::driving_world_assets::{DrivingWorldAssets, DrivingWorldSpawn};

struct Directory(std::path::PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
struct Fixture {
    runtime: CarRuntime,
    _directory: Directory,
}
impl Fixture {
    fn new() -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = Directory(
            std::env::temp_dir().join(format!("lhr-car-lifecycle-{}-{unique}", std::process::id())),
        );
        let mission = directory.0.join("missions/synthetic");
        std::fs::create_dir_all(&mission).unwrap();
        let mut model = b"4DS\0".to_vec();
        model.extend(29u16.to_le_bytes());
        model.extend([0; 13]);
        std::fs::write(mission.join("scene.4ds"), model).unwrap();
        std::fs::write(mission.join("scene2.bin"), [0x53, 0x4c, 6, 0, 0, 0]).unwrap();
        let mut collision = vec![0; 36 + 104];
        collision[..4].copy_from_slice(b"GifC");
        for (offset, value) in [(4, 5_u32), (8, 36), (12, 1), (24, 28), (28, 1)] {
            collision[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        collision[32] = b'f';
        for (index, value) in [0_f32, 0., 10., 10., 10., 10.].into_iter().enumerate() {
            collision[36 + index * 4..40 + index * 4].copy_from_slice(&value.to_le_bytes());
        }
        for offset in [24, 28] {
            collision[36 + offset..40 + offset].copy_from_slice(&1_u32.to_le_bytes());
        }
        collision[36 + 0x20..40 + 0x20].copy_from_slice(&10_f32.to_le_bytes());
        for value in [0_f32, 10., 0., 10.] {
            collision.extend_from_slice(&value.to_le_bytes());
        }
        collision.extend_from_slice(&[0; 24]);
        std::fs::write(mission.join("tree.klz"), collision).unwrap();
        let installation = lhr_assets::Installation::open(&directory.0).unwrap();
        let world = DrivingWorldAssets::load(&installation, "synthetic").unwrap();
        let mut runtime = match CarRuntime::open(
            installation,
            "models/missing.4ds",
            CarRuntimeOptions::default(),
        ) {
            Ok(_) => panic!("missing model accepted"),
            Err(failure) => *failure.partial,
        };
        // Admission-only state over real host owners and synthetic bytes. This
        // does not claim that original construction, Init or GameInit ran.
        let mut car = vec![0; 0x2204];
        car[0x70 + 0xc2c] = 1;
        car[0x70 + 0x4aa] = 1; // Direct braking needs no synthetic ramp parameters.
        car[0x70 + 0x3a4..0x70 + 0x3a8].copy_from_slice(&0.125_f32.to_le_bytes());
        runtime.car = Some(
            VehicleInitState::from_car_bytes(
                CAR,
                car,
                CarInitScratch {
                    address: INIT_STACK,
                    bytes: [0x7d; CAR_INIT_STACK_BYTES],
                },
                CarInitRegisters {
                    ebx: 1,
                    ebp: 2,
                    esi: 3,
                    edi: 4,
                },
            )
            .unwrap(),
        );
        runtime.summary.constructed = true;
        runtime.summary.initialized = true;
        runtime.driving = Some(driving::DrivingSession {
            world,
            listener_sector: None,
            spawn: DrivingWorldSpawn {
                position: [0.; 3],
                rotation_wxyz: [1., 0., 0., 0.],
                scale: [1.; 3],
                actor_name: "synthetic".into(),
                model: "models/missing.4ds".into(),
            },
            mission: "synthetic".into(),
            render_started: false,
            simulation: driving::simulation::SimulationState::default(),
            engine_started: false,
            services_started: false,
            actor_cleanup: OperationState::NotStarted,
            failed: false,
        });
        Self {
            runtime,
            _directory: directory,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    car: Option<Vec<u8>>,
    init_scratch: Option<CarInitScratch>,
    game_scratch: Vec<u8>,
    game_stack: u32,
    game_registers: CarInitRegisters,
    summary: CarRuntimeSummary,
    operations: [OperationState; 3],
    first_calls: [bool; 4],
    session: Option<(bool, bool, bool, bool, OperationState, u64)>,
    allocations: Vec<(u32, usize)>,
    table_allocations: Vec<u32>,
    audio_counts: (usize, usize),
    precision: bool,
}
fn snapshot(runtime: &CarRuntime) -> Snapshot {
    Snapshot {
        car: runtime.car.as_ref().and_then(VehicleInitState::car_bytes),
        init_scratch: runtime
            .car
            .as_ref()
            .and_then(VehicleInitState::car_init_scratch)
            .cloned(),
        game_scratch: runtime.game_scratch.memory.bytes.to_vec(),
        game_stack: runtime.game_scratch.stack,
        game_registers: runtime.game_scratch.registers,
        summary: runtime.summary.clone(),
        operations: [
            runtime.game_init,
            runtime.game_done,
            runtime.car_destruction,
        ],
        first_calls: [
            runtime.level_table_started,
            runtime.wheel_preview_started,
            runtime.wheel_preview_failed,
            runtime.shutdown_started,
        ],
        session: runtime.driving.as_ref().map(|session| {
            (
                session.render_started,
                session.engine_started,
                session.services_started,
                session.failed,
                session.actor_cleanup,
                session.world.render_revision(),
            )
        }),
        allocations: runtime.memory.allocations().collect(),
        table_allocations: runtime.services.table_allocations.clone(),
        audio_counts: (
            runtime.services.audio.live_sound_count().unwrap(),
            runtime.services.audio.live_source_count().unwrap(),
        ),
        precision: runtime.precision.precision_raised(),
    }
}

#[test]
fn stopped_owners_reject_gameplay_before_any_observable_mutation() {
    use OperationState::{Complete, Started};
    for stopped in 0..8 {
        let mut fixture = Fixture::new();
        let runtime = &mut fixture.runtime;
        match stopped {
            0 => runtime.car_destruction = Started,
            1 => runtime.car_destruction = Complete,
            2 => runtime.game_done = Started,
            3 => runtime.game_done = Complete,
            4 => runtime.driving.as_mut().unwrap().actor_cleanup = Started,
            5 => runtime.driving.as_mut().unwrap().actor_cleanup = Complete,
            6 => runtime.shutdown_started = true,
            7 => runtime.summary.initialized = false,
            _ => unreachable!(),
        }
        let before = snapshot(runtime);
        assert_eq!(
            runtime.initialize_game().unwrap_err(),
            "GameInit requires a fresh live initialized car"
        );
        assert_eq!(snapshot(runtime), before, "GameInit: {stopped}");
        assert_eq!(
            runtime.start_driving("synthetic").err().unwrap(),
            "Driving requires a fresh GameInit context"
        );
        assert_eq!(snapshot(runtime), before, "start driving: {stopped}");
        assert_eq!(
            runtime.load_level_table().unwrap_err(),
            "Level tables require a live initialized car and a fresh table call"
        );
        assert_eq!(snapshot(runtime), before, "level table: {stopped}");
        assert_eq!(
            runtime.preview_wheel_steering(0.25, 0.016).unwrap_err(),
            "Wheel preview requires a live initialized car without a previous preview failure"
        );
        assert_eq!(snapshot(runtime), before, "wheel preview: {stopped}");
        runtime.game_init = Complete;
        let before = snapshot(runtime);
        assert_eq!(
            runtime
                .driving_step(
                    16,
                    CarDrivingInput {
                        throttle: 0.75,
                        ..Default::default()
                    },
                )
                .unwrap_err(),
            "Driving requires a live car with completed GameInit"
        );
        assert_eq!(snapshot(runtime), before, "driving: {stopped}");
    }
}

#[test]
fn live_control_failure_retains_the_original_input_prefix() {
    let mut fixture = Fixture::new();
    let runtime = &mut fixture.runtime;
    runtime.game_init = OperationState::Complete;
    assert_eq!(
        runtime
            .driving_step(
                16,
                CarDrivingInput {
                    throttle: 0.75,
                    steer: 2.0,
                    ..Default::default()
                },
            )
            .unwrap_err(),
        "Driving steering input rejected"
    );
    let car = runtime.car.as_ref().unwrap().car_bytes().unwrap();
    assert_eq!(&car[0x70 + 0x3a4..0x70 + 0x3a8], &0.75_f32.to_le_bytes());
    assert!(runtime.driving.as_ref().unwrap().failed);
    let before = snapshot(runtime);
    assert_eq!(
        runtime
            .driving_step(16, CarDrivingInput::default())
            .unwrap_err(),
        "Driving stopped at an interrupted simulation prefix"
    );
    assert_eq!(snapshot(runtime), before);
}

#[test]
fn failed_game_init_cannot_replay_but_can_enter_ordered_cleanup() {
    let mut fixture = Fixture::new();
    let runtime = &mut fixture.runtime;
    // Missing real table inputs stop the admitted call after its start marker.
    let failure = runtime.initialize_game().unwrap_err();
    assert_ne!(failure, "GameInit requires a fresh live initialized car");
    assert_eq!(runtime.game_init, OperationState::Started);
    let before = snapshot(runtime);
    assert_eq!(
        runtime.initialize_game().unwrap_err(),
        "GameInit requires a fresh live initialized car"
    );
    assert_eq!(snapshot(runtime), before);
    assert!(runtime
        .destruct_car()
        .unwrap_err()
        .starts_with("Car GameDone:"));
    assert_eq!(runtime.game_done, OperationState::Started);
    assert_eq!(runtime.car_destruction, OperationState::NotStarted);
    let before = snapshot(runtime);
    assert_eq!(
        runtime.destruct_car().unwrap_err(),
        "GameDone already started"
    );
    assert_eq!(snapshot(runtime), before);
}

#[test]
fn actor_cleanup_failure_closes_admission_before_game_done_or_destruction() {
    let mut fixture = Fixture::new();
    let runtime = &mut fixture.runtime;
    runtime.driving.as_mut().unwrap().services_started = true;
    runtime.car = None;
    assert_eq!(
        runtime.destruct_car().unwrap_err(),
        "Temporary actors have no car caller"
    );
    assert_eq!(
        runtime.driving.as_ref().unwrap().actor_cleanup,
        OperationState::Started
    );
    assert_eq!(runtime.game_done, OperationState::NotStarted);
    assert_eq!(runtime.car_destruction, OperationState::NotStarted);
    let before = snapshot(runtime);
    assert_eq!(
        runtime.initialize_game().unwrap_err(),
        "GameInit requires a fresh live initialized car"
    );
    assert_eq!(
        runtime.destruct_car().unwrap_err(),
        "Temporary actor cleanup retained an interrupted prefix"
    );
    assert_eq!(snapshot(runtime), before);
}

#[test]
fn interrupted_car_destructor_blocks_new_work_and_cannot_replay() {
    let mut fixture = Fixture::new();
    let runtime = &mut fixture.runtime;
    // The synthetic car has no admitted native Init tail scratch. Its actual
    // destructor retains the started state and reports that missing storage.
    assert_eq!(
        runtime.destruct_car().unwrap_err(),
        "Car destruction: NoStorage"
    );
    assert_eq!(runtime.car_destruction, OperationState::Started);
    let before = snapshot(runtime);
    assert_eq!(
        runtime.initialize_game().unwrap_err(),
        "GameInit requires a fresh live initialized car"
    );
    assert_eq!(
        runtime.destruct_car().unwrap_err(),
        "Car destruction already started"
    );
    assert_eq!(snapshot(runtime), before);
}

#[test]
fn completed_host_destruction_still_allows_shared_owner_shutdown() {
    let mut fixture = Fixture::new();
    let runtime = &mut fixture.runtime;
    // Exercise actual host teardown of the retained missing-model context;
    // this is not a claim of a complete native vehicle destructor execution.
    runtime.car = None;
    runtime.summary.constructed = false;
    runtime.summary.initialized = false;
    runtime.destruct_car().unwrap();
    assert_eq!(runtime.car_destruction, OperationState::Complete);
    let before = snapshot(runtime);
    assert_eq!(
        runtime.destruct_car().unwrap_err(),
        "Car destruction already started"
    );
    assert_eq!(snapshot(runtime), before);
    let cleanup = runtime.shutdown().unwrap();
    assert_eq!(cleanup.heap_allocations, 0);
    assert_eq!(cleanup.graphics, 0);
    assert_eq!(cleanup.textures, 0);
    assert_eq!(cleanup.cpu_allocations, 0);
    assert_eq!(cleanup.driver_scene_registrations, 0);
    assert_eq!(cleanup.driver_scene_capacity, 0);
    assert_eq!(runtime.car_destruction, OperationState::Complete);
    assert!(runtime.shutdown_started);
    assert_eq!(
        runtime.shutdown().unwrap_err(),
        "Car inspection cleanup already started"
    );
}
