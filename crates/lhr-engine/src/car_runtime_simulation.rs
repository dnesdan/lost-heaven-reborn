//! Original simulation composition over one borrowed car, frame and world owner.
//! No worker gets a copied vehicle or a host-integrated replacement position.
use crate::{
    collision_query::{CollisionLineOutputs, CollisionLineRequest, CollisionTraversalState},
    driving_world_assets::DrivingWorldAssets,
    native_transform::{self, RetainedVectorAngle, SQuat},
    runtime_audio::RuntimeAudio,
    vehicle_bodycol::VehicleBodycolBackend,
    vehicle_contacts::*,
    vehicle_drivetrain::VehicleDrivetrainState,
    vehicle_dynamics::{self, VehicleEngineBackend},
    vehicle_dynamics_body::*,
    vehicle_dynamics_body_collision::*,
    vehicle_dynamics_body_response::*,
    vehicle_dynamics_tail::VehicleMoveTailBackend,
    vehicle_dynamics_tick::*,
    vehicle_dynamics_update::*,
    vehicle_frame_backend::VehicleFrameBackend,
    vehicle_init::*,
    vehicle_sound::VehicleGearSoundGlobals,
    vehicle_wheel_pose::VehicleWheelPoseState,
};
#[path = "car_runtime_simulation_crash.rs"]
mod crash;
#[path = "car_runtime_simulation_dynamic.rs"]
mod dynamic;
#[path = "car_runtime_simulation_glass.rs"]
mod glass;
#[path = "car_runtime_simulation_glass_lifecycle.rs"]
mod glass_lifecycle;
#[path = "car_runtime_simulation_glass_resources.rs"]
mod glass_resources;
#[path = "car_runtime_simulation_spatial_sound.rs"]
mod spatial_sound;
fn failure(error: impl std::fmt::Debug) -> String {
    format!("{error:?}")
}

#[derive(Default)]
pub(crate) struct SimulationState {
    pub glass_pending: Vec<crate::break_plain_glass::GlassPendingResource>,
    pub game_sound_pending: Vec<crate::game_spatial_sound::GameSoundPending>,
    glass_init_started: bool,
    glass_cleanup_started: bool,
    pub crash_pending: Vec<crate::crash_actors::CrashPendingResource>,
    pub crash_pieces: Vec<crate::crash_actors::CrashPieceObservation>,
    /// Explicit standalone caller-stack fill for the empty allocator byte.
    pub allocator_marker: u8,
    /// Original process atexit registrations, retained in registration order.
    destructors: Vec<u32>,
    cleanup_started: bool,
    /// Bounded diagnostic observations; never read by a simulation worker.
    pub contact_queries: Vec<(CollisionLineRequest, CollisionLineOutputs, Option<u32>)>,
    pub traversal: CollisionTraversalState,
    pub sphere_traversal: crate::collision_swept_world::SphereTraversalState,
}
impl SimulationState {
    pub(crate) fn with_allocator_marker(fill: u8) -> Self {
        Self {
            allocator_marker: fill,
            ..Self::default()
        }
    }
    /// Original process-exit callbacks run in reverse registration order. A
    /// failed cleanup retains its prefix and cannot be replayed.
    pub(crate) fn cleanup<B: VehicleInitBackend>(&mut self, b: &mut B) -> Result<(), String>
    where
        B::Error: std::fmt::Debug,
    {
        if self.cleanup_started {
            return Err("Simulation exit callbacks already started".into());
        }
        self.cleanup_started = true;
        while let Some(&entry) = self.destructors.last() {
            let header = match entry {
                0x4ea9c0 => 0x63d9ec,
                0x4ea9e0 => 0x63d9fc,
                _ => return Err(format!("Unknown simulation exit callback {entry:#x}")),
            };
            let allocation = b.read_word(header + 4).map_err(failure)?;
            b.free(allocation).map_err(failure)?;
            for offset in [4, 8, 12] {
                b.write_word(header + offset, 0).map_err(failure)?;
            }
            self.destructors.pop();
        }
        Ok(())
    }
}
/// Unrecovered external game effects remain typed, ordered host calls. The
/// backend reports their frontier until a reconstructed service is installed.
pub(crate) struct CarRuntimeSimulationBackend<'a, 'b, B: VehicleComponentTailBackend> {
    frames: &'a mut VehicleFrameBackend<'b, B>,
    memory: VehicleComponentTailMemory<'a>,
    world: &'a mut DrivingWorldAssets,
    audio: RuntimeAudio,
    state: &'a mut SimulationState,
}
impl<'a, 'b, B> CarRuntimeSimulationBackend<'a, 'b, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    pub(crate) fn new(
        frames: &'a mut VehicleFrameBackend<'b, B>,
        memory: VehicleComponentTailMemory<'a>,
        world: &'a mut DrivingWorldAssets,
        audio: RuntimeAudio,
        state: &'a mut SimulationState,
    ) -> Self {
        Self {
            frames,
            memory,
            world,
            audio,
            state,
        }
    }
    pub(crate) fn start_engine(&mut self, vehicle: u32) -> Result<bool, String> {
        crate::vehicle_engine_state::vehicle_set_engine_on(self, vehicle, true, true)
    }
    pub(crate) fn tick(&mut self, vehicle: u32, milliseconds: u32) -> Result<(), String> {
        self.state.contact_queries.clear();
        vehicle_tick(self, vehicle, milliseconds).map_err(failure)
    }
    pub(crate) fn update(&mut self, vehicle: u32, milliseconds: u32) -> Result<(), String> {
        update_vehicle(
            self,
            vehicle,
            milliseconds,
            VehicleUpdateGlobals::SUPPLIED_PC,
        )
        .map(|_| ())
        .map_err(failure)
    }
    fn volume(&mut self) -> Result<f32, String> {
        self.read_word(0x634b00).map(f32::from_bits)
    }
    fn steering(&mut self, dt: f32) -> Result<(), String> {
        VehicleWheelPoseState::from_bytes(self.memory.vehicle_bytes_mut())
            .update_steering_wheels(dt, self.frames)
            .map_err(failure)
    }
    fn motor_rot(&mut self, dt: f32, force: f32) -> Result<(), String> {
        let mut direction = self.read_byte(0x634cd4)?;
        let mut globals = VehicleGearSoundGlobals {
            random_seed: self.read_word(0x63a200)?,
            volume_bits: self.read_word(0x634b00)?,
        };
        let result = VehicleDrivetrainState::from_bytes_mut(self.memory.vehicle_bytes_mut())
            .motor_rot_with_gear_sound(dt, force, &mut direction, &mut globals, &mut self.audio)
            .map_err(failure);
        self.write_byte(0x634cd4, direction)?;
        self.write_word(0x63a200, globals.random_seed)?;
        result
    }
    fn vector(&mut self, a: u32) -> Result<[f32; 3], String> {
        Ok([
            f32::from_bits(self.read_word(a)?),
            f32::from_bits(self.read_word(a + 4)?),
            f32::from_bits(self.read_word(a + 8)?),
        ])
    }
    fn update_world(&mut self, frame: u32) -> Result<(), String> {
        self.frames.bodycol_update_world(frame).map_err(failure)
    }
    fn clear_order(&mut self, header: u32) -> Result<(), String> {
        let begin = self.read_word(header + 4)?;
        self.write_word(header + 8, begin)
    }

    fn damage(&mut self, v: u32, strength: f32) -> Result<(), String> {
        use crate::vehicle_dynamics_prepare as math;
        let add = |a, b| math::add::<String>(a, b).map_err(failure);
        let sub = |a, b| math::sub::<String>(a, b).map_err(failure);
        let mul = |a, b| math::mul::<String>(a, b).map_err(failure);
        let div = |a, b| math::div::<String>(a, b).map_err(failure);
        let get = |s: &mut Self, o: u32| {
            math::number::<String>(f32::from_bits(s.read_word(v + o)?)).map_err(failure)
        };
        math::number::<String>(strength).map_err(failure)?;
        if get(self, 0x1c4)? < strength && self.read_word(v + 0xf8)? & 4 == 0 {
            let loss = mul(
                div(
                    sub(strength, mul(mul(get(self, 0x1bc)?, 0.01)?, strength)?)?,
                    get(self, 0x1c0)?,
                )?,
                get(self, 0x1b8)?,
            )?;
            let health = sub(get(self, 0x1b4)?, loss)?;
            self.write_word(v + 0x1b4, health.to_bits())?;
            if health < 0. {
                self.write_word(v + 0x1b4, 0)?;
            }
            let flags = self.read_word(v + 0xf8)?;
            self.write_word(v + 0xf8, flags | 4)?;
            let scale = add(
                mul(
                    sub(1., get(self, 0x1ac)?)?,
                    div(get(self, 0x1b4)?, get(self, 0x1b8)?)?,
                )?,
                get(self, 0x1ac)?,
            )?;
            self.write_word(v + 0x1b0, scale.to_bits())?;
        }
        if get(self, 0x1b4)? <= 0. {
            self.write_byte(v + 0x5f0, 1)?;
        }
        if get(self, 0x1fc)? < strength && self.read_word(v + 0xf8)? & 8 == 0 {
            let loss = mul(
                div(
                    sub(strength, mul(mul(get(self, 0x1f4)?, 0.01)?, strength)?)?,
                    get(self, 0x1f8)?,
                )?,
                get(self, 0x1f0)?,
            )?;
            let health = sub(get(self, 0x1ec)?, loss)?;
            self.write_word(v + 0x1ec, health.to_bits())?;
            if health < 0. {
                self.write_word(v + 0x1ec, 0)?;
            }
            let flags = self.read_word(v + 0xf8)?;
            self.write_word(v + 0xf8, flags | 8)?;
        }
        Ok(())
    }
    fn wheel_skid(&mut self, v: u32, index: u32, emit: bool) -> Result<(), String> {
        if emit {
            crate::vehicle_runtime_effects::vehicle_wheel_skid(self, v, index).map_err(failure)
        } else {
            crate::vehicle_runtime_effects::vehicle_end_skid_addressable(self, v, index)
                .map_err(failure)
        }
    }
    fn frame_effect(
        &mut self,
        e: crate::vehicle_runtime_effects::FrameEffect,
    ) -> Result<(), String> {
        use crate::vehicle_runtime_effects::FrameEffect;
        match e {
            FrameEffect::SetOn { frame, enabled } => self
                .frames
                .set_axis_frame_on(frame, u32::from(enabled))
                .map_err(failure),
            FrameEffect::Update { frame } => {
                let key = self.frames.frame_key(frame).map_err(failure)?;
                self.frames.arena_mut().graph.update(key).map_err(failure)
            }
            FrameEffect::Scale { frame, scale } => {
                let key = self.frames.frame_key(frame).map_err(failure)?;
                self.frames
                    .arena_mut()
                    .graph
                    .assign_scale(key, scale)
                    .map_err(failure)
            }
            FrameEffect::SetDir { frame, direction } => self
                .frames
                .set_axis_frame_direction(frame, direction.map(f32::to_bits))
                .map_err(failure),
            FrameEffect::MatrixDir3 {
                frame,
                direction,
                up,
            } => self
                .frames
                .set_axis_matrix_direction(frame, direction.map(f32::to_bits), up.map(f32::to_bits))
                .map_err(failure),
            other => Err(format!("Vehicle mesh effect {other:?}")),
        }
    }
}
impl<B> VehicleInitBackend for CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    type Error = String;
    fn read_byte(&mut self, a: u32) -> Result<u8, String> {
        self.memory
            .read_byte_with_backend(self.frames, a)
            .map_err(failure)
    }
    fn read_word(&mut self, a: u32) -> Result<u32, String> {
        self.memory
            .read_word_with_backend(self.frames, a)
            .map_err(failure)
    }
    fn write_byte(&mut self, a: u32, v: u8) -> Result<(), String> {
        self.memory
            .write_byte_with_backend(self.frames, a, v)
            .map_err(failure)
    }
    fn write_word(&mut self, a: u32, v: u32) -> Result<(), String> {
        self.memory
            .write_word_with_backend(self.frames, a, v)
            .map_err(failure)
    }
    fn allocate(&mut self, n: u32) -> Result<u32, String> {
        self.frames
            .allocate_projected(&mut self.memory, n)
            .map_err(failure)
    }
    fn free(&mut self, a: u32) -> Result<(), String> {
        self.frames
            .free_projected(&mut self.memory, a)
            .map_err(failure)
    }
    fn collect_frame(&mut self, v: &mut VehicleInitFrameVector, f: u32) -> Result<(), String> {
        self.frames.collect_frame(v, f).map_err(failure)
    }
    fn enumerate(
        &mut self,
        f: u32,
        q: VehicleInitQuery,
        _e: &mut VehicleInitEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<String>> {
        Err(VehicleInitUnavailable::Effect(format!(
            "Simulation enumeration {f:#x} {q:?}"
        )))
    }
}
impl<B> VehicleTickBackend for CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    fn tick_effect(&mut self, v: u32, e: VehicleTickEffect) -> Result<(), String> {
        match e {
            VehicleTickEffect::SteeringWheels { seconds } => self.steering(seconds),
            VehicleTickEffect::EngineSound { enabled } => {
                let audio = self.audio.clone();
                let volume = self.volume()?;
                crate::runtime_audio::vehicle::vehicle_engine_snd(self, v, enabled, &audio, volume)
                    .map_err(failure)
            }
            VehicleTickEffect::ProcedureEbe40 { seconds } => {
                let audio = self.audio.clone();
                let mut volume = self.volume()?;
                crate::runtime_audio::vehicle::vehicle_dynamic_engine_sound(
                    self,
                    v,
                    seconds,
                    &audio,
                    &mut volume,
                )
                .map_err(failure)
            }
            VehicleTickEffect::ProcedureEd910 => {
                let audio = self.audio.clone();
                let volume = self.volume()?;
                crate::runtime_audio::vehicle::vehicle_audio_ed910(self, v, &audio, volume)
                    .map_err(failure)
            }
            VehicleTickEffect::ProcedureEeb50 => {
                let audio = self.audio.clone();
                crate::runtime_audio::vehicle::vehicle_stop_engine_loops(self, v, &audio)
                    .map_err(failure)
            }
        }
    }
    fn tick_set_gear(&mut self, _v: u32, g: i32) -> Result<(), String> {
        crate::vehicle_controls::VehicleControlState::from_bytes_mut(
            self.memory.vehicle_bytes_mut(),
        )
        .set_gear(g)
        .map(|_| ())
        .map_err(failure)
    }
    fn tick_motor_force(&mut self, _v: u32, dt: f32, p: f32) -> Result<f32, String> {
        VehicleDrivetrainState::from_bytes_mut(self.memory.vehicle_bytes_mut())
            .motor_force(dt, p)
            .map_err(failure)
    }
    fn tick_motor_rot(&mut self, _v: u32, dt: f32, f: f32) -> Result<(), String> {
        self.motor_rot(dt, f)
    }
    fn tick_engine(&mut self, v: u32, dt: f32, a: f32, b: f32) -> Result<(), String> {
        vehicle_dynamics::vehicle_engine(self, v, dt, a, b)
            .map(|_| ())
            .map_err(failure)
    }
    fn tick_maximum_moment(
        &mut self,
        v: u32,
        r: f32,
        g: i32,
        reverse: bool,
    ) -> Result<Option<f32>, String> {
        vehicle_get_max_moment_by_rot(self, v, r, g, reverse, |s, _, rotation, power| {
            VehicleDrivetrainState::from_bytes_mut(s.memory.vehicle_bytes_mut())
                .compute_moment(rotation, power)
                .map_err(failure)
        })
        .map_err(failure)
    }
}
impl<B> VehicleEngineBackend for CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    type Angle = RetainedVectorAngle;
    fn engine_move(&mut self, v: u32, a: [f32; 6]) -> Result<(), String> {
        crate::vehicle_dynamics_move::vehicle_move(self, v, a)
            .map(|_| ())
            .map_err(failure)
    }
    fn engine_angle_to(&mut self, a: [f32; 3], b: [f32; 3]) -> Result<Self::Angle, String> {
        native_transform::try_retained_vector_angle(a, b).map_err(failure)
    }
    fn engine_angle_spill(&mut self, a: &Self::Angle) -> Result<f32, String> {
        a.spill().map_err(failure)
    }
    fn engine_angle_ratio(&mut self, a: Self::Angle, n: f32) -> Result<f32, String> {
        a.divide_numerator(n).map_err(failure)
    }
}
impl<B> VehicleMoveTailBackend for CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    fn move_update_world_matrix(&mut self, f: u32) -> Result<(), String> {
        self.update_world(f)
    }
    fn move_angle_to(&mut self, a: [f32; 3], b: [f32; 3]) -> Result<f32, String> {
        native_transform::try_vector_angle_to(a, b).map_err(failure)
    }
    fn move_set_steer(&mut self, _v: u32, x: f32) -> Result<(), String> {
        crate::vehicle_controls::VehicleControlState::from_bytes_mut(
            self.memory.vehicle_bytes_mut(),
        )
        .set_steer(x)
        .map(|_| ())
        .map_err(failure)
    }
    fn move_update_steering_wheels(&mut self, _v: u32, dt: f32) -> Result<(), String> {
        self.steering(dt)
    }
    fn move_wheels_collision(
        &mut self,
        v: u32,
        dt: f32,
        down: [f32; 3],
        impact: bool,
    ) -> Result<[f32; 3], String> {
        {
            let mut output = [0.; 3];
            vehicle_do_wheels_collision(self, v, dt, down, impact, &mut output).map_err(failure)?;
            Ok(output)
        }
    }
    fn move_motor_rot(&mut self, _v: u32, dt: f32, f: f32) -> Result<(), String> {
        self.motor_rot(dt, f)
    }
}
impl<B> VehicleContactsBackend for CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    fn contact_primitive_properties(&mut self, p: u32) -> Result<u32, String> {
        if let Some(source) = self.world.collision().primitive(p) {
            Ok(source.properties)
        } else {
            self.read_word(p)
        }
    }
    fn contact_primitive_material(&mut self, p: u32) -> Result<u8, String> {
        if let Some(source) = self.world.collision().primitive(p) {
            Ok(source.material())
        } else {
            self.read_byte(p.wrapping_add(3))
        }
    }
    fn contact_test_line_v(
        &mut self,
        r: CollisionLineRequest,
        o: &mut CollisionLineOutputs,
    ) -> Result<Option<VehicleContactHit>, String> {
        let (mut query, min_y) = crate::collision_query::prepare_line_query(&r).map_err(failure)?;
        crate::dynamic_collision_queries::line_in_dynamics(
            self,
            0x647f48,
            r.selector as u32,
            &mut query,
        )
        .map_err(failure)?;
        let hit = crate::collision_query::test_line_v_grid_continue(
            self.world.collision(),
            &r,
            o,
            &mut self.state.traversal,
            query,
            min_y,
        )
        .map_err(failure)?;
        if self.state.contact_queries.len() < 64 {
            self.state
                .contact_queries
                .push((r, *o, hit.map(|h| h.primitive)));
        }
        Ok(hit.map(|h| VehicleContactHit {
            primitive: h.primitive,
        }))
    }

    fn contact_filter(&mut self, f: u32, c: u32, p: u32, w: u32) -> Result<u32, String> {
        if f != 0x426d90 {
            return Err(format!("Wheel contact callback {f:#x} context {c:#x}"));
        }
        crate::vehicle_car_contacts::car_wheel_contact_filter(self, c, p, w).map_err(failure)
    }

    fn contact_update_frame_world(&mut self, f: u32) -> Result<(), String> {
        self.update_world(f)
    }
    fn contact_frame_direction(&mut self, f: u32) -> Result<[f32; 3], String> {
        use crate::vehicle_dynamics_prepare as math;
        let add = |a, b| math::add::<String>(a, b).map_err(failure);
        let mul = |a, b| math::mul::<String>(a, b).map_err(failure);
        let div = |a, b| math::div::<String>(a, b).map_err(failure);
        let sub = |a, b| math::sub::<String>(a, b).map_err(failure);
        if self.read_word(f + 0xac)? & 0x20 == 0 {
            self.update_world(f)?;
        }
        let mut v = self.vector(f + 0x30)?;
        let squared = add(add(mul(v[0], v[0])?, mul(v[2], v[2])?)?, mul(v[1], v[1])?)?;
        if f64::from(sub(squared, 1.)?.abs()) >= f64::from(1e-8_f32) {
            if squared >= 1e-8_f32 {
                let root = squared.sqrt();
                crate::native_precision::record_sqrt(squared, root);
                let reciprocal = div(1., root)?;
                for x in &mut v {
                    *x = mul(*x, reciprocal)?;
                }
            } else {
                let axis = if v[0] != 0. {
                    0
                } else if v[2] != 0. {
                    2
                } else {
                    1
                };
                v[axis] = if v[axis] < 0. { -1. } else { 1. };
            }
        }
        Ok(v)
    }
    fn contact_effect(&mut self, v: u32, e: VehicleContactEffect<'_>) -> Result<(), String> {
        match e {
            VehicleContactEffect::ImpactParticles {
                point,
                down,
                magnitude,
            } => crate::vehicle_damage::vehicle_do_damage(
                self,
                v,
                &mut crate::vehicle_damage::VehicleDamageCall {
                    point: crate::vehicle_damage::DamageVector::Address(point),
                    direction: crate::vehicle_damage::DamageVector::Address(down),
                    radius: 0.2,
                    strength: magnitude,
                    flags: 0,
                    extra: 0,
                },
            )
            .map_err(failure),
            VehicleContactEffect::ImpactCallback {
                callback,
                context,
                impulse,
                point,
                velocity,
            } => {
                if callback != 0x4259e0 {
                    return Err(format!("Unknown wheel impact callback {callback:#x}"));
                }
                crate::vehicle_car_contacts::car_impact_callback(
                    self,
                    context,
                    crate::vehicle_car_contacts::VehicleCarImpactCall {
                        point: crate::vehicle_car_contacts::VehicleCarImpactPoint::Address(point),
                        contact_impulse: *impulse,
                        velocity,
                        dynamic_owner: 0,
                        descriptor: 0,
                    },
                )
                .map_err(failure)
            }
            VehicleContactEffect::ImpactSound { magnitude } => self.damage(v, magnitude),
            VehicleContactEffect::ImpactDamage => {
                let audio = self.audio.clone();
                let volume = self.volume()?;
                crate::runtime_audio::vehicle::vehicle_impact_voice(self, v, &audio, volume)
                    .map_err(failure)
            }
        }
    }
}
impl<B> VehicleRigidBodyBackend for CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    fn body_collision(&mut self, v: u32, c: &mut VehicleBodyCollisionCall) -> Result<bool, String> {
        vehicle_body_collision(self, v, c).map_err(failure)
    }
}
impl<B> VehicleBodyCollisionBackend for CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    fn body_collision_initialize(&mut self) -> Result<(), String> {
        for (bit, header, destructor) in [(1, 0x63d9fc, 0x4ea9e0), (2, 0x63d9ec, 0x4ea9c0)] {
            let flags = self.read_byte(0x63da0c)?;
            if flags & bit == 0 {
                self.write_byte(0x63da0c, flags | bit)?;
                self.write_byte(header, self.state.allocator_marker)?;
                for offset in [4, 8, 12] {
                    self.write_word(header + offset, 0)?;
                }
                self.state.destructors.push(destructor);
            }
        }
        Ok(())
    }
    fn body_collision_clear_order(&mut self) -> Result<(), String> {
        self.clear_order(0x63d9fc)
    }
    fn body_collision_insert(
        &mut self,
        h: u32,
        position: u32,
        index: u32,
        distance: f32,
    ) -> Result<(), String> {
        crate::vehicle_dynamics_body_collision::insert_collision_order(
            self, h, position, index, distance,
        )
        .map_err(failure)
    }
    fn body_collision_update_world(&mut self, f: u32) -> Result<(), String> {
        self.update_world(f)
    }
    fn body_collision_filter(&mut self, f: VehicleBodyCollisionFilter) -> Result<(), String> {
        crate::vehicle_dynamics_body_collision::set_body_collision_filter(self, f)
    }
    fn body_collision_sphere(&mut self, q: VehicleBodySphereSweep) -> Result<bool, String> {
        self.body_sphere_dynamic_and_static(q)
            .map(|hit| hit.is_some())
    }
    fn body_collision_volume(&mut self, q: VehicleBodyVolumeSweep) -> Result<bool, String> {
        let bounds = [self.vector(q.volume)?, self.vector(q.volume + 12)?];
        let p = crate::collision_swept_box::BoxParam::new(
            &crate::collision_swept_box::BoxSweepRequest {
                bounds,
                first_matrix: q.previous,
                second_matrix: q.current,
                vectors: [q.displacement, q.auxiliary, q.up],
                dynamic_mask: q.mask,
                static_mask: q.flags,
            },
        )
        .map_err(failure)?;
        if p.dynamic_mask != u32::MAX
            && crate::dynamic_collision_queries::box_query::box_in_dynamics(self, 0x647f48, &p)
                .map_err(failure)?
        {
            return Ok(true);
        }
        crate::collision_swept_box::test_d_box_static(self.world.collision(), &p).map_err(failure)
    }
}
impl<B> VehicleBodyCollisionResponseBackend for CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    fn body_collision_clear_fine_order(&mut self) -> Result<(), String> {
        self.clear_order(0x63d9ec)
    }
    fn body_collision_fine_sphere(
        &mut self,
        q: VehicleBodySphereSweep,
    ) -> Result<Option<VehicleBodySweepHit>, String> {
        self.body_sphere_dynamic_and_static(q)
    }
    fn body_collision_map_descriptor(
        &mut self,
        v: u32,
        f: u32,
        c: u32,
        h: &mut VehicleBodySweepHit,
    ) -> Result<u32, String> {
        if f != 0x426340 {
            return Err(format!(
                "Body descriptor callback {f:#x} context {c:#x} vehicle {v:#x}"
            ));
        }
        crate::vehicle_car_contacts::car_body_contact_filter(self, c, h).map_err(failure)
    }

    fn body_collision_nested(
        &mut self,
        v: u32,
        h: u32,
        c: &mut VehicleBodyCollisionCall,
    ) -> Result<bool, String> {
        let outer = c.collision_records;
        c.collision_records = h;
        let result = vehicle_body_collision(self, v, c).map_err(failure);
        c.collision_records = outer;
        result
    }
}
impl<B> crate::vehicle_damage::VehicleDamageMeshBackend for CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    fn damage_virtual(&mut self, object: u32, slot: u32, args: &[u32]) -> Result<u32, String> {
        if args.len() > 4 {
            return Err("Damage mesh virtual argument range".into());
        }
        let mut arguments = [0; 4];
        arguments[..args.len()].copy_from_slice(args);
        self.frames
            .vehicle_resource_effect(
                self.memory.reborrow(),
                VehicleResourceEffect::MeshVirtual {
                    object,
                    slot,
                    arguments,
                    count: args.len() as u8,
                },
            )
            .map_err(failure)
    }
    fn damage_update_world(&mut self, frame: u32) -> Result<(), String> {
        self.update_world(frame)
    }
    fn damage_rand(&mut self) -> Result<i32, String> {
        self.vehicle_update_rand()
    }
    fn damage_notification(
        &mut self,
        _v: u32,
        function: u32,
        context: u32,
        frame: u32,
        part_type: u16,
        point: [f32; 3],
        impulse: [f32; 3],
        changed: bool,
    ) -> Result<(), String> {
        self.frames
            .game_host_damage_notification(
                self.memory.reborrow(),
                crate::vehicle_frame_backend::VehicleFrameDamageNotification {
                    function,
                    context,
                    frame,
                    part_type,
                    point,
                    impulse,
                    changed,
                },
            )
            .map_err(failure)
    }
    fn damage_break_glass(
        &mut self,
        v: u32,
        index: u32,
        direction: [f32; 3],
    ) -> Result<(), String> {
        crate::vehicle_damage_glass::vehicle_damage_glass(self, v, index, direction)
            .map_err(failure)
    }
    fn damage_detach_part(
        &mut self,
        v: u32,
        function: u32,
        context: u32,
        index: u32,
        velocity: [f32; 3],
        extra: u32,
    ) -> Result<(), String> {
        Err(format!("DoDamage part detach {v:#x}, function{function:#x}, context{context:#x}, part{index}, velocity{velocity:?}, extra{extra:#x}"))
    }
    fn damage_lamp_switch(&mut self, v: u32, index: u32, side: u32) -> Result<(), String> {
        Err(format!("DoDamage lamp switch {v:#x}, {index}, {side}"))
    }
}
impl<B> crate::vehicle_damage::VehicleDamageBackend for CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    fn damage_mesh_record(
        &mut self,
        v: u32,
        index: u32,
        _record: u32,
        call: &mut crate::vehicle_damage::VehicleDamageCall,
        effective: f32,
        factor: f32,
    ) -> Result<(), String> {
        crate::vehicle_damage::vehicle_damage_complete_row(self, v, index, call, effective, factor)
            .map_err(failure)
    }
    fn damage_wheel_detach(
        &mut self,
        v: u32,
        function: u32,
        context: u32,
        index: u32,
        velocity: [f32; 3],
        extra: u32,
    ) -> Result<(), String> {
        Err(format!("DoDamage wheel detach {v:#x}, function{function:#x}, context{context:#x}, wheel{index}, velocity{velocity:?}, extra{extra:#x}"))
    }
}
impl<B> crate::vehicle_runtime_effects::VehicleSkidBackend
    for CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    fn skid_frame_effect(
        &mut self,
        e: crate::vehicle_runtime_effects::FrameEffect,
    ) -> Result<(), String> {
        self.frame_effect(e)
    }
    fn skid_primitive_properties(&mut self, p: u32) -> Result<u32, String> {
        self.contact_primitive_properties(p)
    }
}
impl<B> crate::vehicle_car_contacts::VehicleCarContactBackend
    for CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    fn car_contact_crash_model(
        &mut self,
        call: crate::vehicle_car_contacts::VehicleCarCrashCall,
    ) -> Result<(), crate::vehicle_car_contacts::VehicleCarContactUnavailable<String>> {
        use crate::vehicle_car_contacts::VehicleCarContactUnavailable as Error;
        let info = self
            .world
            .collision()
            .primitive(call.descriptor)
            .ok_or_else(|| {
                Error::Effect(format!("Unknown crash descriptor {}", call.descriptor))
            })?;
        let frame = if let Some(binding) = info.bound_volume {
            self.world
                .crash_frame_identity(binding.frame)
                .map_err(Error::Effect)?
        } else {
            return Err(Error::Effect(format!(
                "Crash descriptor {} has no resolved frame owner: {:?}",
                call.descriptor, info.owner_unavailable
            )));
        };
        crate::vehicle_car_contacts::vehicle_create_crash_model(
            self,
            frame,
            call.point,
            call.velocity,
            call.scale,
        )
        .map(|_| ())
        .map_err(|e| Error::Effect(failure(e)))
    }
    fn car_contact_impact_burst(
        &mut self,
        call: crate::vehicle_car_contacts::VehicleCarImpactBurst,
    ) -> Result<(), crate::vehicle_car_contacts::VehicleCarContactUnavailable<String>> {
        use crate::vehicle_car_contacts::VehicleCarContactUnavailable as Error;
        // Original425adb..5ae5 reloads App->Game, then passes its C_Debris+240
        // owner even when GenerateDebris's own count guard will return early.
        let app = self.read_word(0x63788c).map_err(Error::Effect)?;
        let game = self.read_word(app + 0x24).map_err(Error::Effect)?;
        crate::debris::generate_debris(
            self,
            game.wrapping_add(0x240),
            crate::debris::DebrisGeneration {
                point: call.point,
                count: call.count,
                scalars: [
                    f32::from_bits(call.parameters[0]),
                    f32::from_bits(call.parameters[1]),
                    f32::from_bits(call.parameters[2]),
                ],
                velocity: if call.parameters[3] == 0 {
                    None
                } else {
                    Some(call.parameters[3])
                },
            },
        )
        .map_err(|e| Error::Effect(failure(e)))
    }
    fn car_contact_material(&mut self, p: u32) -> Result<u8, String> {
        self.contact_primitive_material(p)
    }
    fn car_contact_properties(&mut self, p: u32) -> Result<u32, String> {
        self.contact_primitive_properties(p)
    }
    fn car_contact_actor_owner(&mut self, p: u32) -> Result<u32, String> {
        self.body_static_descriptor_owner(p)
    }
}
impl<B> crate::vehicle_car_contacts::VehicleCrashModelBackend
    for CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    fn crash_frame_kind(&mut self, frame: u32) -> Result<u32, String> {
        use crate::frame_transform::FrameKind;
        let key = self.world.crash_frame_key(frame)?;
        Ok(match self.world.frame_source(key)?.kind {
            FrameKind::Mesh | FrameKind::Singlemesh | FrameKind::Projector => 1,
            FrameKind::Light => 2,
            FrameKind::Camera => 3,
            FrameKind::Sound => 4,
            FrameKind::Sector => 5,
            FrameKind::Dummy => 6,
            FrameKind::Target => 7,
            FrameKind::Model => 9,
            FrameKind::Joint => 10,
            FrameKind::Scene => return Err("Crash worker needs a source frame, not scene".into()),
        })
    }
    fn crash_frame_parent(&mut self, frame: u32) -> Result<Option<u32>, String> {
        let key = self.world.crash_frame_key(frame)?;
        self.world
            .frame_source(key)?
            .parent
            .map(|p| self.world.crash_frame_identity(p))
            .transpose()
    }
    fn crash_frame_children(&mut self, frame: u32) -> Result<Vec<u32>, String> {
        let key = self.world.crash_frame_key(frame)?;
        self.world
            .frame_children(key)?
            .into_iter()
            .map(|k| self.world.crash_frame_identity(k))
            .collect()
    }
    fn crash_frame_properties(&mut self, frame: u32) -> Result<Vec<u8>, String> {
        let key = self.world.crash_frame_key(frame)?;
        Ok(self
            .world
            .frame_source(key)?
            .properties
            .unwrap_or(&[])
            .to_vec())
    }
    fn crash_model_filename(&mut self, frame: u32) -> Result<Vec<u8>, String> {
        let key = self.world.crash_frame_key(frame)?;
        Ok(self
            .world
            .frame_source(key)?
            .model_resource
            .unwrap_or("")
            .as_bytes()
            .to_vec())
    }
    fn crash_model_collisions_off(&mut self, frame: u32) -> Result<(), String> {
        let key = self.world.crash_frame_key(frame)?;
        self.world.model_collisions_off(key).map(|_| ())
    }
    fn crash_create_actors(
        &mut self,
        call: crate::vehicle_car_contacts::VehicleCreateCrashActorsCall,
    ) -> Result<bool, String> {
        crate::crash_actors::create_crash_actors(self, &call).map_err(failure)
    }
    fn crash_set_on(&mut self, frame: u32, on: bool) -> Result<(), String> {
        let key = self.world.crash_frame_key(frame)?;
        self.world.set_frame_on(key, u32::from(on))
    }
    fn crash_debustize(&mut self, frame: u32) -> Result<(), String> {
        let key = self.world.crash_frame_key(frame)?;
        self.world.debustize(key)
    }
}
impl<B> VehicleBodyStaticResponseBackend for CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    fn body_static_descriptor_properties(&mut self, p: u32) -> Result<u32, String> {
        self.contact_primitive_properties(p)
    }
    fn body_static_descriptor_owner(&mut self, p: u32) -> Result<u32, String> {
        if self.world.collision().primitive(p).is_some() {
            return Err(format!(
                "Dynamic actor owner for static-world descriptor {p}"
            ));
        }
        // Dynamic descriptors are canonical records created by the original
        // car/physical-object initializers; +44 is their actual actor owner.
        self.read_word(p.wrapping_add(0x44))
    }
    fn body_dynamic_flags(
        &mut self,
        v: u32,
        descriptor: u32,
        allowed: &mut bool,
    ) -> Result<bool, VehicleBodyStaticUnavailable<String>> {
        let function = self
            .read_word(v + 0x2f0)
            .map_err(VehicleBodyStaticUnavailable::Effect)?;
        if function != 0x425970 {
            return Err(VehicleBodyStaticUnavailable::Effect(format!(
                "Actor_Filter callback {function:#x}"
            )));
        }
        let context = self
            .read_word(v + 0xc74)
            .map_err(VehicleBodyStaticUnavailable::Effect)?;
        let mut byte = u8::from(*allowed);
        let result = crate::vehicle_car_body_dynamic::car_body_collision_flags(
            self, context, descriptor, &mut byte,
        );
        *allowed = byte != 0;
        result.map_err(|e| VehicleBodyStaticUnavailable::Effect(failure(e)))
    }
    fn body_dynamic_properties(
        &mut self,
        v: u32,
        descriptor: u32,
        point: [f32; 3],
        normal: [f32; 3],
        out: &mut crate::vehicle_car_body_dynamic::VehicleCarOtherBody,
    ) -> Result<bool, VehicleBodyStaticUnavailable<String>> {
        let function = self
            .read_word(v + 0x2ec)
            .map_err(VehicleBodyStaticUnavailable::Effect)?;
        if function != 0x425190 {
            return Err(VehicleBodyStaticUnavailable::Effect(format!(
                "Actor_Col_Info callback {function:#x}"
            )));
        }
        crate::vehicle_car_body_dynamic::car_body_other_properties(
            self, descriptor, point, normal, out,
        )
        .map_err(|e| VehicleBodyStaticUnavailable::Effect(failure(e)))
    }
    fn body_dynamic_apply(
        &mut self,
        v: u32,
        descriptor: u32,
        point: &mut [f32; 3],
        impulse: &mut [f32; 3],
        normal: &mut [f32; 3],
        mass: f32,
    ) -> Result<(), VehicleBodyStaticUnavailable<String>> {
        let function = self
            .read_word(v + 0x2e4)
            .map_err(VehicleBodyStaticUnavailable::Effect)?;
        if function != 0x425480 {
            return Err(VehicleBodyStaticUnavailable::Effect(format!(
                "Call_Collision callback {function:#x}"
            )));
        }
        let context = self
            .read_word(v + 0xc74)
            .map_err(VehicleBodyStaticUnavailable::Effect)?;
        let before = normal.map(f32::to_bits);
        let result = crate::vehicle_car_body_dynamic::car_body_apply_other_impulse(
            self, context, descriptor, point, impulse, normal, mass,
        );
        for (i, x) in normal.iter().copied().enumerate() {
            if x.to_bits() != before[i] {
                self.write_word(0x63d9d8 + i as u32 * 4, x.to_bits())
                    .map_err(VehicleBodyStaticUnavailable::Effect)?;
            }
        }
        result.map_err(|e| VehicleBodyStaticUnavailable::Effect(failure(e)))
    }
    fn body_dynamic_static_notify(
        &mut self,
        v: u32,
        descriptor: u32,
        point: &mut [f32; 3],
        linear: &mut [f32; 3],
        normal: &mut [f32; 3],
    ) -> Result<(), VehicleBodyStaticUnavailable<String>> {
        let function = self
            .read_word(v + 0x2e8)
            .map_err(VehicleBodyStaticUnavailable::Effect)?;
        if function != 0x4256b0 {
            return Err(VehicleBodyStaticUnavailable::Effect(format!(
                "Call_Collision1 callback {function:#x}"
            )));
        }
        let context = self
            .read_word(v + 0xc74)
            .map_err(VehicleBodyStaticUnavailable::Effect)?;
        let previous_linear = linear.map(f32::to_bits);
        let previous_normal = normal.map(f32::to_bits);
        let result = crate::vehicle_car_body_dynamic::car_body_static_notification(
            self, context, descriptor, point, linear, normal,
        );
        for (i, x) in linear.iter().copied().enumerate() {
            if x.to_bits() != previous_linear[i] {
                self.write_word(v + 0x1f90 + i as u32 * 4, x.to_bits())
                    .map_err(VehicleBodyStaticUnavailable::Effect)?;
            }
        }
        for (i, x) in normal.iter().copied().enumerate() {
            if x.to_bits() != previous_normal[i] {
                self.write_word(0x63d9d8 + i as u32 * 4, x.to_bits())
                    .map_err(VehicleBodyStaticUnavailable::Effect)?;
            }
        }
        result.map_err(|e| VehicleBodyStaticUnavailable::Effect(failure(e)))
    }
    fn body_static_effect(
        &mut self,
        v: u32,
        e: &mut VehicleBodyStaticEffect,
    ) -> Result<(), String> {
        match *e {
            VehicleBodyStaticEffect::Damage {
                point,
                normal,
                radius,
                strength,
            } => crate::vehicle_damage::vehicle_do_damage(
                self,
                v,
                &mut crate::vehicle_damage::VehicleDamageCall {
                    point: crate::vehicle_damage::DamageVector::Value(point),
                    direction: crate::vehicle_damage::DamageVector::Value(normal),
                    radius,
                    strength,
                    flags: 0,
                    extra: 0,
                },
            )
            .map_err(failure),
            VehicleBodyStaticEffect::Callback {
                function,
                context,
                contact_impulse,
                point,
                velocity,
                dynamic_owner,
                descriptor,
                ..
            } => {
                if function != 0x4259e0 {
                    return Err(format!("Unknown body impact callback {function:#x}"));
                }
                crate::vehicle_car_contacts::car_impact_callback(
                    self,
                    context,
                    crate::vehicle_car_contacts::VehicleCarImpactCall {
                        point: crate::vehicle_car_contacts::VehicleCarImpactPoint::Value(point),
                        contact_impulse,
                        velocity,
                        dynamic_owner,
                        descriptor,
                    },
                )
                .map_err(failure)
            }
            VehicleBodyStaticEffect::Sound { strength, grounded } => {
                let audio = self.audio.clone();
                let volume = self.volume()?;
                crate::runtime_audio::vehicle::vehicle_collision_sound(
                    self,
                    v,
                    strength,
                    grounded != 0,
                    &audio,
                    volume,
                )
                .map_err(failure)
            }
            VehicleBodyStaticEffect::AdditionalDamage { strength } => self.damage(v, strength),
        }
    }
    fn body_static_damage_timer(
        &mut self,
        v: u32,
        ms: u32,
        p: [f32; 3],
        m: u32,
    ) -> Result<bool, String> {
        crate::vehicle_damage::vehicle_damage_timer(self, v, ms, p, m).map_err(failure)
    }
}
impl<B> VehicleUpdateBackend for CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    fn vehicle_update_effect(&mut self, v: u32, e: VehicleUpdateEffect) -> Result<(), String> {
        match e {
            VehicleUpdateEffect::ModelUpdate { frame } => {
                let key = self.frames.frame_key(frame).map_err(failure)?;
                self.frames.arena_mut().graph.update(key).map_err(failure)
            }
            VehicleUpdateEffect::ScenePosition { frame, position } => {
                let p = self.vector(position)?;
                let scene_address = self.read_word(0x63d9d4)?;
                let scene = crate::scene_ownership::SceneKey(
                    self.frames.frame_key(scene_address).map_err(failure)?,
                );
                let key = self.frames.frame_key(frame).map_err(failure)?;
                self.frames
                    .arena_mut()
                    .set_frame_sector_pos(scene, key, p)
                    .map(|_| ())
                    .map_err(failure)
            }
            VehicleUpdateEffect::WheelSkid { index, emit } => self.wheel_skid(v, index, emit),
            VehicleUpdateEffect::DynamicUpdate { object } => {
                crate::dynamic_collision_manager::dyn_update(self, 0x647f48, object)
                    .map_err(failure)
            }
            VehicleUpdateEffect::SetGear { direction } => self.tick_set_gear(v, direction as i32),
            VehicleUpdateEffect::SteeringVisual {
                frame,
                direction,
                angle,
            } => {
                let direction = self.vector(direction)?;
                let rotation =
                    SQuat::try_make_frame_direction(direction, angle).map_err(failure)?;
                let key = self.frames.frame_key(frame).map_err(failure)?;
                let state = self
                    .frames
                    .arena_mut()
                    .graph
                    .projected_state_mut(key)
                    .map_err(failure)?;
                state.rotation = rotation;
                state.flags = (state.flags & 0xffff_fecb) | 0x4000_0008;
                Ok(())
            }
            VehicleUpdateEffect::Windows { seconds } => {
                crate::vehicle_runtime_effects::vehicle_windows(self, v, seconds, |b, e| {
                    b.frame_effect(e)
                })
                .map_err(failure)
            }
            VehicleUpdateEffect::Strips { seconds } => {
                crate::vehicle_runtime_effects::vehicle_strips(self, v, seconds, |b, e| {
                    b.frame_effect(e)
                })
                .map_err(failure)
            }
            VehicleUpdateEffect::WheelParticles { milliseconds } => {
                crate::vehicle_runtime_effects::vehicle_wheel_particles(self, v, milliseconds)
                    .map_err(failure)
            }
            VehicleUpdateEffect::ExhaustParticles { milliseconds } => {
                crate::vehicle_runtime_effects::vehicle_exhaust_particles(self, v, milliseconds)
                    .map_err(failure)
            }
            VehicleUpdateEffect::Doors { seconds } => self
                .frames
                .game_vehicle_dependency(
                    self.memory.reborrow(),
                    v,
                    0x4d7750,
                    [seconds.to_bits(), 0],
                )
                .map(|_| ())
                .map_err(failure),
            VehicleUpdateEffect::Lights { seconds } => self
                .frames
                .game_vehicle_dependency(
                    self.memory.reborrow(),
                    v,
                    0x4d7cd0,
                    [seconds.to_bits(), 0],
                )
                .map(|_| ())
                .map_err(failure),
            VehicleUpdateEffect::Axes => self
                .frames
                .game_vehicle_dependency(self.memory.reborrow(), v, 0x4d6840, [0, 0])
                .map(|_| ())
                .map_err(failure),
            VehicleUpdateEffect::SteeringWheels { seconds } => self.steering(seconds),
            VehicleUpdateEffect::RotationWheels { seconds } => {
                VehicleWheelPoseState::from_bytes(self.memory.vehicle_bytes_mut())
                    .update_rotation_wheels(seconds, self.frames)
                    .map_err(failure)
            }
            VehicleUpdateEffect::Horn { enabled } => {
                let audio = self.audio.clone();
                let volume = self.volume()?;
                crate::runtime_audio::vehicle::vehicle_horn(self, v, enabled != 0, &audio, volume)
                    .map_err(failure)
            }
            VehicleUpdateEffect::Siren { enabled } => {
                let audio = self.audio.clone();
                let volume = self.volume()?;
                crate::runtime_audio::vehicle::vehicle_siren(self, v, enabled != 0, &audio, volume)
                    .map_err(failure)
            }
            VehicleUpdateEffect::SkidSound { seconds } => {
                let audio = self.audio.clone();
                let volume = self.volume()?;
                crate::runtime_audio::vehicle::vehicle_skid_sound(self, v, seconds, &audio, volume)
                    .map_err(failure)
            }
            VehicleUpdateEffect::EnvironmentSound => {
                let audio = self.audio.clone();
                let volume = self.volume()?;
                crate::runtime_audio::vehicle::vehicle_environment_sound(self, v, &audio, volume)
                    .map_err(failure)
            }
            VehicleUpdateEffect::FlatTyreSound { audible } => {
                let audio = self.audio.clone();
                let volume = self.volume()?;
                crate::runtime_audio::vehicle::vehicle_flat_tyre_sound(
                    self, v, audible, &audio, volume,
                )
                .map_err(failure)
            }
        }
    }
    fn vehicle_scene_sample(&mut self, p: u32) -> Result<f32, String> {
        let position = self.vector(p)?;
        let scene_address = self.read_word(0x63d9d4)?;
        let scene = crate::scene_ownership::SceneKey(
            self.frames.frame_key(scene_address).map_err(failure)?,
        );
        crate::scene_lightness::scene_lightness(self.frames.arena(), scene, position)
            .map_err(failure)
    }
    fn vehicle_motor_particles(&mut self, v: u32, ms: u32) -> Result<bool, String> {
        crate::vehicle_runtime_effects::vehicle_motor_particles(self, v, ms).map_err(failure)
    }
    fn vehicle_update_rand(&mut self) -> Result<i32, String> {
        let next = self
            .read_word(0x63a200)?
            .wrapping_mul(214013)
            .wrapping_add(2531011);
        self.write_word(0x63a200, next)?;
        Ok(((next >> 16) & 0x7fff) as i32)
    }
    fn vehicle_update_make_rotation(&mut self, a: [f32; 3], r: f32) -> Result<SQuat, String> {
        SQuat::try_make_axis_angle_reduced(a, r).map_err(failure)
    }
    fn vehicle_update_normalize_rotation(&mut self, f: u32) -> Result<(), String> {
        self.frames.normalize_frame_quaternion(f).map_err(failure)
    }
}

impl<B> crate::vehicle_engine_state::VehicleEngineStateBackend
    for CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    fn engine_state_sound(&mut self, v: u32, enabled: bool) -> Result<(), String> {
        let audio = self.audio.clone();
        let volume = self.volume()?;
        crate::runtime_audio::vehicle::vehicle_engine_snd(self, v, enabled, &audio, volume)
            .map_err(failure)
    }
    fn engine_state_failed_start_sound(&mut self, v: u32) -> Result<(), String> {
        let audio = self.audio.clone();
        let volume = self.volume()?;
        crate::runtime_audio::vehicle::vehicle_failed_start_sound(self, v, &audio, volume)
            .map_err(failure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle_frame_backend::VehicleFrameMemory;

    #[test]
    fn process_exit_frees_both_orders_and_preserves_allocator_markers() {
        let mut memory = VehicleFrameMemory::default();
        memory.map_bytes(0x63d9ec, &[0xa7; 32]).unwrap();
        let coarse = memory.allocate(16).unwrap();
        let fine = memory.allocate(8).unwrap();
        for (header, allocation, size) in [(0x63d9ec, fine, 8), (0x63d9fc, coarse, 16)] {
            memory.write_word(header + 4, allocation).unwrap();
            memory.write_word(header + 8, allocation + size).unwrap();
            memory.write_word(header + 12, allocation + size).unwrap();
        }
        let mut state = SimulationState::with_allocator_marker(0x39);
        state.destructors = vec![0x4ea9e0, 0x4ea9c0];
        state.cleanup(&mut memory).unwrap();
        assert_eq!(memory.allocations().count(), 0);
        for header in [0x63d9ec, 0x63d9fc] {
            assert_eq!(memory.read_word(header).unwrap(), 0xa7a7a7a7);
            for offset in [4, 8, 12] {
                assert_eq!(memory.read_word(header + offset).unwrap(), 0);
            }
        }
        assert!(state.cleanup(&mut memory).is_err());
    }

    #[test]
    fn failed_exit_keeps_the_earlier_registration_owned_and_cannot_replay() {
        let mut memory = VehicleFrameMemory::default();
        memory.map_bytes(0x63d9ec, &[0; 32]).unwrap();
        let coarse = memory.allocate(16).unwrap();
        memory.write_word(0x63da00, coarse).unwrap();
        memory.write_word(0x63d9f0, 0x12345678).unwrap();
        let mut state = SimulationState {
            destructors: vec![0x4ea9e0, 0x4ea9c0],
            ..SimulationState::default()
        };
        assert!(state.cleanup(&mut memory).is_err());
        assert_eq!(memory.allocations().count(), 1);
        assert_eq!(memory.read_word(0x63da00).unwrap(), coarse);
        assert_eq!(state.destructors, vec![0x4ea9e0, 0x4ea9c0]);
        assert!(state.cleanup(&mut memory).is_err());
    }
}
