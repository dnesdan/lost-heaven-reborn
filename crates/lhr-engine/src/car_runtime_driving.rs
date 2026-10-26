//! Live driving composition. Simulation uses the recovered vehicle workers;
//! the render view never advances or substitutes body motion.
use super::*;
use crate::vehicle_frame_backend::VehicleFrameGameHost;
use crate::{driving_world_assets::DrivingWorldAssets, runtime_audio::RuntimeAudio};
#[path = "car_runtime_simulation.rs"]
pub(crate) mod simulation;

#[derive(Clone, Copy, Debug, Default)]
pub struct CarDrivingInput {
    pub throttle: f32,
    pub brake: f32,
    pub steer: f32,
    pub handbrake: bool,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct CarDrivingState {
    pub position: [f32; 3],
    pub speed: f32,
    pub contacts: u32,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct CarDrivingControlFeedback {
    pub throttle: f32,
    pub brake: f32,
    pub steering_requested_radians: f32,
    pub steering_applied_radians: f32,
    pub steering_limit_radians: f32,
    pub handbrake_force: f32,
    pub gear: i32,
}
#[derive(Default)]
pub(super) struct CarRuntimeServices {
    pub audio: RuntimeAudio,
    pub table_allocations: Vec<u32>,
    pub allocator_fill: u8,
}
#[path = "car_runtime_services.rs"]
mod services;
pub(super) struct DrivingSession {
    pub world: DrivingWorldAssets,
    pub listener_sector: Option<crate::scene_ownership::SectorKey>,
    pub spawn: crate::driving_world_assets::DrivingWorldSpawn,
    pub mission: String,
    pub render_started: bool,
    pub simulation: simulation::SimulationState,
    pub engine_started: bool,
    pub services_started: bool,
    pub actor_cleanup_started: bool,
    pub actor_cleanup_complete: bool,
    pub failed: bool,
}
impl CarRuntime {
    pub(super) fn cleanup_driving_actors_inner(&mut self) -> Result<(), String> {
        let Some(session) = self.driving.as_mut() else {
            return Ok(());
        };
        // No actor/model factory is reached before the first simulation call.
        if !session.services_started || session.actor_cleanup_complete {
            return Ok(());
        }
        if session.actor_cleanup_started {
            return Err("Temporary actor cleanup retained an interrupted prefix".into());
        }
        session.actor_cleanup_started = true;
        // Real mesh access retains driver VB/IB locks until publication, even
        // when the simulation stopped before its normal end-of-frame boundary.
        self.arena
            .publish_runtime_buffers()
            .map_err(|e| format!("Temporary resource publication: {e:?}"))?;
        let audio = self.services.audio.clone();
        let car = self
            .car
            .as_mut()
            .ok_or("Temporary actors have no car caller")?;
        let mut frames = VehicleFrameBackend::with_installation(
            &mut self.arena,
            &mut self.memory,
            &self.installation,
        )
        .with_car_track_resources(&mut self.tracks)
        .with_game_init_stack(GAME_STACK)
        .with_game_host(&mut self.services);
        let mut backend = simulation::CarRuntimeSimulationBackend::new(
            &mut frames,
            car.car_live_memory(&mut self.game_scratch.memory)
                .ok_or("Temporary actors have no canonical car projection")?,
            &mut session.world,
            audio,
            &mut session.simulation,
        );
        backend.cleanup_temporary_actors()?;
        session.actor_cleanup_complete = true;
        Ok(())
    }
    pub(super) fn prepare_driving_world(
        &mut self,
        path: &str,
        mission: &str,
    ) -> Result<(), String> {
        let world = DrivingWorldAssets::load(&self.installation, mission)?;
        let spawn = world.authored_driving_spawn(path)?;
        self.driving = Some(DrivingSession {
            world,
            listener_sector: None,
            spawn,
            mission: mission.into(),
            render_started: false,
            simulation: simulation::SimulationState::with_allocator_marker(
                self.services.allocator_fill,
            ),
            engine_started: false,
            services_started: false,
            actor_cleanup_started: false,
            actor_cleanup_complete: false,
            failed: false,
        });
        Ok(())
    }
    /// Standalone original-data caller context. This invokes actual GameInit
    /// and retains any interrupted prefix; it is never replayed on failure.
    pub fn initialize_game(&mut self) -> Result<(), String> {
        Pc24PrecisionState::run_owner(self, |o| &mut o.precision, |o| o.initialize_game_inner())
    }
    fn initialize_game_inner(&mut self) -> Result<(), String> {
        if self.game_init_started || !self.summary.initialized || self.shutdown_started {
            return Err("GameInit requires a fresh live initialized car".into());
        }
        self.game_init_started = true;
        self.load_driving_tables()?;
        let scene = self.scene.ok_or("GameInit has no scene owner")?;
        let scene_address = {
            let frames = VehicleFrameBackend::new(&mut self.arena, &mut self.memory);
            frames
                .frame_address(scene.root())
                .map_err(|e| format!("GameInit scene identity: {e:?}"))?
        };
        // The standalone caller owns these empty managers. They are explicit
        // host context, not a copied native mission-construction result.
        self.memory
            .map_bytes(0x680000, &vec![0; 0x4000])
            .map_err(|e| format!("GameInit caller context: {e:?}"))?;
        crate::game_spatial_sound::initialize_sound_context(
            &mut self.memory,
            0x680000,
            self.services.allocator_fill,
        )
        .map_err(|e| format!("Game sound caller context: {e:?}"))?;
        crate::break_plain_glass_lifecycle::glass_construct(
            &mut self.memory,
            0x680208,
            self.services.allocator_fill,
        )
        .map_err(|e| format!("Game glass caller context: {e:?}"))?;
        self.memory
            .map_bytes(0x63d9d4, &scene_address.to_le_bytes())
            .map_err(|e| format!("GameInit scene context: {e:?}"))?;
        for (address, bytes) in [
            (0x647f48, vec![0; 0x2c0]),
            (0x63d9d8, vec![0; 0x38]),
            (0x63d880, vec![0; 16]),
            (0x63d8a0, vec![0; 16]),
            (0x634cd4, vec![1]),
            (0x634cd8, b"PNEU_PUNC.WAV\0".to_vec()),
            (0x63d6c4, vec![0]),
            (0x634b00, 1f32.to_bits().to_le_bytes().to_vec()),
            (0x6d4560, vec![0; 8]),
            (0x623614, 0x447a0000u32.to_le_bytes().to_vec()),
            (0x62348c, 0x3c23d70au32.to_le_bytes().to_vec()),
            (0x624e48, 0x3c75c28fu32.to_le_bytes().to_vec()),
            (0x6232fc, 0x3f000000u32.to_le_bytes().to_vec()),
            (0x6233e4, 0x3e99999au32.to_le_bytes().to_vec()),
        ] {
            self.memory
                .map_bytes(address, &bytes)
                .map_err(|e| format!("GameInit simulation context {address:#x}: {e:?}"))?;
        }
        if let Some(session) = self.driving.as_ref() {
            crate::dynamic_collision_manager::initialize_dynamic_grid(
                &mut self.memory,
                0x647f48,
                session.world.dynamic_grid_source(),
            )
            .map_err(|e| format!("Driving dynamic grid: {e:?}"))?;
        }
        VehicleInitBackend::write_word(&mut self.memory, 0x647df8 + 0x10, scene_address)
            .map_err(|e| format!("GameInit scene: {e:?}"))?;
        VehicleInitBackend::write_word(&mut self.memory, 0x647df8 + 0x24, 0x680000)
            .map_err(|e| format!("GameInit caller: {e:?}"))?;
        let sectors = self.arena.scene(scene).map_err(|e| e.to_string())?.sectors;
        let sector_words = {
            let frames = VehicleFrameBackend::new(&mut self.arena, &mut self.memory);
            let mut bytes = Vec::new();
            for sector in sectors {
                bytes.extend_from_slice(
                    &frames
                        .frame_address(sector.frame())
                        .map_err(|e| format!("GameInit sector: {e:?}"))?
                        .to_le_bytes(),
                );
            }
            bytes
        };
        let sectors = self.own_game_table(&sector_words)?;
        for (address, value) in [
            (0x63d894, sectors),
            (0x63d898, sectors + sector_words.len() as u32),
            (0x63d89c, sectors + sector_words.len() as u32),
        ] {
            VehicleInitBackend::write_word(&mut self.memory, address, value)
                .map_err(|e| format!("GameInit sector list: {e:?}"))?;
        }
        let sound_sectors = self.own_game_table(&sector_words)?;
        for (address, value) in [
            (0x63d884, sound_sectors),
            (0x63d888, sound_sectors + sector_words.len() as u32),
            (0x63d88c, sound_sectors + sector_words.len() as u32),
        ] {
            VehicleInitBackend::write_word(&mut self.memory, address, value)
                .map_err(|e| format!("GameInit sound sector list: {e:?}"))?;
        }
        let mut frames = VehicleFrameBackend::with_installation(
            &mut self.arena,
            &mut self.memory,
            &self.installation,
        )
        .with_car_track_resources(&mut self.tracks)
        .with_game_init_stack(GAME_STACK)
        .with_game_host(&mut self.services);
        let mut backend = game::CarRuntimeGameBackend::new(
            &mut frames,
            &self.installation,
            &mut self.game,
            GAME_STACK,
        );
        self.car
            .as_mut()
            .ok_or("GameInit has no car")?
            .car_game_init(&mut backend, &mut self.game_scratch)
            .map_err(|e| format!("Car GameInit: {e:?}"))?;
        self.arena
            .publish_runtime_buffers()
            .map_err(|e| format!("GameInit frame publication: {e:?}"))?;
        self.game_initialized = true;
        Ok(())
    }
    pub fn start_driving(&mut self, mission: &str) -> Result<lhr_render::Scene, String> {
        Pc24PrecisionState::run_owner(
            self,
            |o| &mut o.precision,
            |o| o.start_driving_inner(mission),
        )
    }
    fn start_driving_inner(&mut self, mission: &str) -> Result<lhr_render::Scene, String> {
        if self.game_init_started {
            return Err("Driving requires a fresh GameInit context".into());
        }
        let session = self
            .driving
            .as_mut()
            .ok_or("Driving requires open_driving so its authored pose precedes Init")?;
        if session.render_started {
            return Err("Driving already started".into());
        }
        if !session.mission.eq_ignore_ascii_case(mission) {
            return Err("Driving mission differs from its prepared world".into());
        }
        session.render_started = true;
        session
            .world
            .load_cache_render_near(&self.installation, session.spawn.position, 300.)?;
        let scene = session
            .world
            .render_scene_near(session.spawn.position, 200.)?;
        self.initialize_game_inner()?;
        Ok(scene)
    }
    pub fn driving_step(
        &mut self,
        milliseconds: u32,
        input: CarDrivingInput,
    ) -> Result<CarDrivingState, String> {
        Pc24PrecisionState::run_owner(
            self,
            |o| &mut o.precision,
            |o| o.driving_step_inner(milliseconds, input),
        )
    }
    fn driving_step_inner(
        &mut self,
        milliseconds: u32,
        input: CarDrivingInput,
    ) -> Result<CarDrivingState, String> {
        if !self.game_initialized || self.shutdown_started {
            return Err("Driving requires completed GameInit".into());
        }
        let session = self.driving.as_mut().ok_or("Driving has no world owner")?;
        if session.failed {
            return Err("Driving stopped at an interrupted simulation prefix".into());
        }
        session.failed = true;
        let car = self.car.as_mut().ok_or("Driving has no car owner")?;
        {
            let mut controls = car.controls();
            let accept =
                |name: &str,
                 result: Result<bool, crate::vehicle_controls::VehicleControlUnavailable>|
                 -> Result<(), String> {
                    if result.map_err(|e| format!("Driving {name}: {e:?}"))? {
                        Ok(())
                    } else {
                        Err(format!("Driving {name} input rejected"))
                    }
                };
            accept("throttle", controls.set_power(input.throttle, true))?;
            accept("brake", controls.set_brake(input.brake))?;
            accept("steering", controls.set_steer(input.steer))?;
            accept("handbrake", controls.set_handbrake(input.handbrake))?;
        }
        {
            let audio = self.services.audio.clone();
            let mut frames = VehicleFrameBackend::with_installation(
                &mut self.arena,
                &mut self.memory,
                &self.installation,
            )
            .with_car_track_resources(&mut self.tracks)
            .with_game_init_stack(GAME_STACK)
            .with_game_host(&mut self.services);
            let mut backend = simulation::CarRuntimeSimulationBackend::new(
                &mut frames,
                car.car_live_memory(&mut self.game_scratch.memory)
                    .ok_or("Driving canonical car projection unavailable")?,
                &mut session.world,
                audio,
                &mut session.simulation,
            );
            if !session.engine_started {
                session.services_started = true;
                backend.prepare_glass_services()?;
                backend.prepare_crash_services()?;
                backend.start_engine()?;
                backend.set_gear(1)?;
                session.engine_started = true;
            }
            backend.prepare_temporary_actors()?;
            backend
                .tick(milliseconds)
                .map_err(|e| format!("Driving Tick: {e}"))?;
            backend
                .update(milliseconds)
                .map_err(|e| format!("Driving Update: {e}"))?;
            backend
                .tick_temporary_actors(milliseconds)
                .map_err(|e| format!("Driving temporary actors: {e}"))?;
            backend.tick_glass(milliseconds)?;
            backend.tick_game_sounds(milliseconds)?;
            crate::dynamic_collision_manager::dyn_optimize_tick(&mut backend, 0x647f48)
                .map_err(|e| format!("Driving world tick: {e:?}"))?;
        }
        self.arena
            .publish_runtime_buffers()
            .map_err(|e| format!("Driving frame publication: {e:?}"))?;
        let state = current_state(car, &mut self.memory, &mut self.arena, self.model)?;
        session.failed = false;
        Ok(state)
    }
    /// Reads the current canonical prefix even after a stopped simulation call.
    pub fn driving_state(&mut self) -> Result<CarDrivingState, String> {
        Pc24PrecisionState::run_owner(
            self,
            |o| &mut o.precision,
            |o| {
                current_state(
                    o.car.as_ref().ok_or("Driving has no car owner")?,
                    &mut o.memory,
                    &mut o.arena,
                    o.model,
                )
            },
        )
    }

    pub fn driving_mix_audio(
        &mut self,
        sample_rate: u32,
        output: &mut [f32],
    ) -> Result<(), String> {
        crate::vehicle_frame_backend::sync_runtime_audio_frames(
            &self.services.audio,
            &mut self.arena,
        )
        .map_err(|e| e.to_string())?;
        self.services
            .audio
            .mix_stereo(sample_rate, output)
            .map_err(|e| e.to_string())
    }
    /// Use the same scene sector query as Camera::UpdateV at the drawn camera's
    /// actual PC-coordinate position, retaining its previous sector as a hint.
    pub fn driving_set_audio_listener(
        &mut self,
        position: [f32; 3],
        right: [f32; 3],
    ) -> Result<(), String> {
        let scene = self.scene.ok_or("Driving listener has no scene owner")?;
        let session = self
            .driving
            .as_mut()
            .ok_or("Driving listener has no world owner")?;
        sync_scene_audio_listener(
            &self.services.audio,
            &mut self.arena,
            &mut self.memory,
            scene,
            &mut session.listener_sector,
            position,
            right,
        )
    }

    pub fn driving_audio(&self) -> RuntimeAudio {
        self.services.audio.clone()
    }
    /// Read-only feedback from the original setters' canonical destination fields.
    pub fn driving_control_feedback(&self) -> Result<CarDrivingControlFeedback, String> {
        let bytes = self
            .car
            .as_ref()
            .ok_or("Driving has no car owner")?
            .as_bytes();
        let word = |a: usize| u32::from_le_bytes(bytes[a..a + 4].try_into().unwrap());
        Ok(CarDrivingControlFeedback {
            throttle: f32::from_bits(word(0x3a4)),
            brake: f32::from_bits(word(0x5b8)),
            steering_requested_radians: f32::from_bits(word(0x624)),
            steering_applied_radians: f32::from_bits(word(0x628)),
            steering_limit_radians: f32::from_bits(word(0x610)),
            handbrake_force: f32::from_bits(word(0x434)),
            gear: word(0x55c) as i32,
        })
    }
    /// The actual primary-sector parent of the live car, shadow and skid models.
    pub fn driving_scene_root(&self) -> Result<crate::frame_transform::FrameKey, String> {
        let scene = self.scene.ok_or("Driving has no scene owner")?;
        Ok(self.arena.scene(scene).map_err(|e| e.to_string())?.sectors[0].frame())
    }
    /// Read-only revision of the live static graph's rendering state.
    pub fn driving_world_render_revision(&self) -> Result<u64, String> {
        Ok(self
            .driving
            .as_ref()
            .ok_or("Driving has no world owner")?
            .world
            .render_revision())
    }
    pub fn driving_render_world_near(
        &mut self,
        position: [f32; 3],
        radius: f32,
    ) -> Result<lhr_render::Scene, String> {
        let session = self.driving.as_mut().ok_or("Driving has no world owner")?;
        session
            .world
            .load_cache_render_near(&self.installation, position, radius + 100.)?;
        session.world.render_scene_near(position, radius)
    }
    /// Read-only diagnostics of reached physical-piece construction.
    pub fn driving_crash_pieces(
        &self,
    ) -> Result<Vec<crate::crash_actors::CrashPieceObservation>, String> {
        Ok(self
            .driving
            .as_ref()
            .ok_or("Driving has no world owner")?
            .simulation
            .crash_pieces
            .clone())
    }
    /// Current line-query diagnostics, with opaque source primitive identities.
    pub fn driving_contact_queries(&self) -> Result<String, String> {
        Ok(format!(
            "{:?}",
            self.driving
                .as_ref()
                .ok_or("Driving has no world owner")?
                .simulation
                .contact_queries
        ))
    }
    pub fn driving_wheel_diagnostics(&mut self) -> Result<Vec<[u32; 6]>, String> {
        let car = self.car.as_ref().ok_or("Driving has no car owner")?;
        let bytes = car.as_bytes();
        let word = |a: usize| u32::from_le_bytes(bytes[a..a + 4].try_into().unwrap());
        let count = word(0x4c4);
        if count > 1024 {
            return Err("Driving wheel diagnostics exceed their storage budget".into());
        }
        let mut rows = Vec::new();
        for i in 0..count {
            let wheel = VehicleInitBackend::read_word(&mut self.memory, word(0xc38) + i * 4)
                .map_err(|e| format!("Driving wheel: {e:?}"))?;
            let mut row = [0; 6];
            for (out, offset) in row.iter_mut().zip([0xa8, 0xac, 0xb0, 0xb4, 0x120, 0x124]) {
                *out = VehicleInitBackend::read_word(&mut self.memory, wheel + offset)
                    .map_err(|e| format!("Driving wheel parameters: {e:?}"))?;
            }
            rows.push(row);
        }
        Ok(rows)
    }
    pub(super) fn finish_game(&mut self) -> Result<(), String> {
        if self.game_done_started {
            return Err("GameDone already started".into());
        }
        self.game_done_started = true;
        let mut frames = VehicleFrameBackend::with_installation(
            &mut self.arena,
            &mut self.memory,
            &self.installation,
        )
        .with_car_track_resources(&mut self.tracks)
        .with_game_init_stack(GAME_STACK)
        .with_game_host(&mut self.services);
        let mut backend = game::CarRuntimeGameBackend::new(
            &mut frames,
            &self.installation,
            &mut self.game,
            GAME_STACK,
        );
        self.car
            .as_mut()
            .ok_or("GameDone has no car")?
            .car_game_done(&mut backend, &mut self.game_scratch)
            .map_err(|e| format!("Car GameDone: {e:?}"))?;
        self.arena
            .publish_runtime_buffers()
            .map_err(|e| format!("GameDone frame publication: {e:?}"))?;
        self.game_done_complete = true;
        Ok(())
    }
}

fn current_state(
    car: &VehicleInitState,
    memory: &mut VehicleFrameMemory,
    arena: &mut ModelArena,
    model: Option<ModelKey>,
) -> Result<CarDrivingState, String> {
    let bytes = car.as_bytes();
    let word = |offset: usize| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    let wheels = word(0x4c4);
    if wheels > 1024 {
        return Err("Driving wheel diagnostics exceed their storage budget".into());
    }
    let mut contacts = 0;
    for i in 0..wheels {
        let wheel = VehicleInitBackend::read_word(memory, word(0xc38) + i * 4)
            .map_err(|e| format!("Driving wheel read: {e:?}"))?;
        contacts += u32::from(
            VehicleInitBackend::read_word(memory, wheel + 0x120)
                .map_err(|e| format!("Driving contact read: {e:?}"))?
                & 8
                != 0,
        );
    }
    let model = model.ok_or("Driving has no live model")?;
    let position = arena
        .world_matrix(model.root())
        .map_err(|e| e.to_string())?
        .try_transform_point([0.; 3])
        .map_err(|e| format!("Driving position: {e:?}"))?;
    Ok(CarDrivingState {
        position,
        speed: f32::from_bits(word(0x59c)),
        contacts,
    })
}

fn sync_scene_audio_listener(
    audio: &RuntimeAudio,
    arena: &mut ModelArena,
    memory: &mut VehicleFrameMemory,
    scene: SceneKey,
    hint: &mut Option<crate::scene_ownership::SectorKey>,
    position: [f32; 3],
    right: [f32; 3],
) -> Result<(), String> {
    if !position.iter().chain(right.iter()).all(|v| v.is_finite()) {
        return Err("Nonfinite driving listener".into());
    }
    let sector = arena
        .scene_get_sector(scene, position, *hint)
        .map_err(|e| format!("Driving listener sector: {e:?}"))?;
    let address = VehicleFrameBackend::new(arena, memory)
        .frame_address(sector.frame())
        .map_err(|e| format!("Driving listener sector address: {e:?}"))?;
    audio
        .set_listener(position, right, Some(address))
        .map_err(|e| e.to_string())?;
    *hint = Some(sector);
    Ok(())
}

#[cfg(test)]
mod audio_listener_tests {
    use super::*;

    #[test]
    fn listener_sector_tracks_actual_camera_position_and_preserves_hint_on_error() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let primary = arena.scene(scene).unwrap().sectors[0];
        let sector = arena.create_sector_frame().unwrap();
        arena
            .set_frame_bound_volume(sector.frame(), [[-1.0; 3], [1.0; 3]])
            .unwrap();
        arena.sector_add_to_scene_ring(sector, Some(scene)).unwrap();
        let audio = RuntimeAudio::default();
        let mut memory = VehicleFrameMemory::default();
        let mut hint = None;
        sync_scene_audio_listener(
            &audio,
            &mut arena,
            &mut memory,
            scene,
            &mut hint,
            [0.0; 3],
            [1.0, 0.0, 0.0],
        )
        .unwrap();
        assert_eq!(hint, Some(sector));
        // The following call cannot replace the camera or its last valid hint.
        assert!(sync_scene_audio_listener(
            &audio,
            &mut arena,
            &mut memory,
            scene,
            &mut hint,
            [f32::NAN, 0.0, 0.0],
            [1.0, 0.0, 0.0]
        )
        .is_err());
        assert_eq!(hint, Some(sector));
        sync_scene_audio_listener(
            &audio,
            &mut arena,
            &mut memory,
            scene,
            &mut hint,
            [2.0; 3],
            [1.0, 0.0, 0.0],
        )
        .unwrap();
        assert_eq!(hint, Some(primary));
    }
}
