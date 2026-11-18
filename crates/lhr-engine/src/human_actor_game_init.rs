//! Game.exe RVA 0x174bd0..0x174f2a and CreateDynColls RVA 0x175ca0..0x175db1.
//! Native evidence: ev_5d169f16ff7b57d2468fd0ea808c4f41ed830f5194ccaa62b890ad1e17d059eb;
//! animation Init RVA 0xe660: ev_9e427bced6823b12c8eb0fd6f9c5d91fe293dde6cd37d03a5f2d38d94b63a0e1.
//! CreateDynColls: ev_9699e7dd0c98580946b29d0909ceb62e0160d7f4798cd02f5997d8e6b6194d19.
//! DynInsert: ev_d3055cdc531cac464d14158f51520fcc242819e9da038062417ba0f35952009f.
//! World-cache thunk: ev_46d36d2cfae614ac329d8aab0447b6effb55563dd89f918d9a42cc85368f2934;
//! allocation: ev_f401aacca9b1377f7bcd74b453d4d4a1607bcbc3e3c398ce050e404586b25625.
//! The binary hash is recorded by the enclosing module. The reference names
//! come from mafia-re fe7e7464; this phase earns no full-function credit.
//! No local floating arithmetic: literal stores and bit-preserving copies need
//! no x87 policy. World-cache arithmetic remains in the actual frame owner.
#![forbid(unsafe_code)]

use super::{HumanState, ANIM_BREATH01A, HUMAN_ACTION_STAND, HUMAN_STEP_END};
use crate::animation_pose::PoseHandle;
use crate::script_scheduler::{ActorReferenceId, FrameReferenceId};
use std::cell::RefCell;
use std::collections::TryReserveError;
use std::rc::Rc;

/// One authoritative carried-frame slot. Legacy boolean staging cannot invent
/// a frame identity; a real frame setter replaces that unresolved presence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HumanCarriedFrame {
    Absent,
    UnresolvedPresence,
    Frame(FrameReferenceId),
}

impl HumanCarriedFrame {
    pub fn is_present(self) -> bool {
        self != Self::Absent
    }
}

/// Untouched constructor storage is not a fabricated initialized value.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum HumanGameInitValue<T> {
    #[default]
    Unestablished,
    Established(T),
}

/// Only the native null store is established. Non-null traffic, collision-cell
/// and matrix ownership need their own actual resources, not invented handles.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HumanNullReference {
    #[default]
    Unestablished,
    Null,
}

/// Actual shared collision resource, not the legacy HumanDynCollId capture ID.
pub type HumanDynamicCollisionHandle = Rc<RefCell<HumanDynamicCollision>>;

/// Known members of the native 0x48-byte allocation. Names follow the reference
/// tDynamicCollObject; unknown_20 has no inferred meaning. Cell and matrices are
/// only established as NULL by this prefix, before the insertion boundary.
/// Sphere and box coordinates use native world units; copies preserve bits.
#[derive(Debug, PartialEq)]
pub struct HumanDynamicCollision {
    pub kind: u32,
    pub frame: Option<FrameReferenceId>,
    pub flags: u32,
    pub cell: HumanNullReference,
    pub sphere_position: [f32; 3],
    pub sphere_radius: f32,
    pub unknown_20: u32,
    pub bounding_box: [[f32; 3]; 2],
    pub matrix: HumanNullReference,
    pub inverse_matrix: HumanNullReference,
    pub owner: Option<ActorReferenceId>,
}

impl HumanDynamicCollision {
    pub fn zeroed() -> Self {
        Self {
            kind: 0,
            frame: None,
            flags: 0,
            cell: HumanNullReference::Null,
            sphere_position: [0.0; 3],
            sphere_radius: 0.0,
            unknown_20: 0,
            bounding_box: [[0.0; 3]; 2],
            matrix: HumanNullReference::Null,
            inverse_matrix: HumanNullReference::Null,
            owner: None,
        }
    }
}

/// Heap allocation of collision-reference slots. Untouched allocator storage
/// stays Unestablished until the reached native clear or slot write.
#[derive(Debug)]
pub struct HumanDynamicCollisionSlots {
    pub slots: Box<[HumanGameInitValue<Option<HumanDynamicCollisionHandle>>]>,
}

impl HumanDynamicCollisionSlots {
    pub fn try_allocate(count: u32) -> Result<Self, TryReserveError> {
        Ok(Self {
            slots: allocate_unestablished_slots(count)?,
        })
    }
}

/// The same allocation semantics for frame references; notably the second
/// frame slot is not cleared by the lower-collision prefix.
#[derive(Debug)]
pub struct HumanCollisionFrameSlots {
    pub slots: Box<[HumanGameInitValue<Option<FrameReferenceId>>]>,
}

impl HumanCollisionFrameSlots {
    pub fn try_allocate(count: u32) -> Result<Self, TryReserveError> {
        Ok(Self {
            slots: allocate_unestablished_slots(count)?,
        })
    }
}

fn allocate_unestablished_slots<T>(
    count: u32,
) -> Result<Box<[HumanGameInitValue<T>]>, TryReserveError> {
    let mut slots = Vec::new();
    slots.try_reserve_exact(count as usize)?;
    slots.resize_with(count as usize, || HumanGameInitValue::Unestablished);
    Ok(slots.into_boxed_slice())
}

/// The permanent Human member owner, not a decoded snapshot or second registry.
/// Established(None) records an actual allocation NULL, not missing evidence.
#[derive(Debug, Default)]
pub struct HumanDynamicCollisions {
    pub num_colls: HumanGameInitValue<u32>,
    pub colls: HumanGameInitValue<Option<HumanDynamicCollisionSlots>>,
    pub coll_frames: HumanGameInitValue<Option<HumanCollisionFrameSlots>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HumanDynCollsStorageError {
    UnestablishedCount,
    UnestablishedCollisionSlots,
    NullCollisionSlots,
    CollisionSlotOutOfBounds { index: u32, len: usize },
    UnestablishedCollision { index: u32 },
    NullCollision { index: u32 },
    CollisionBorrowConflict { index: u32 },
    UnestablishedFrameSlots,
    NullFrameSlots,
    FrameSlotOutOfBounds { index: u32, len: usize },
    UnestablishedFrame { index: u32 },
}

/// Exactly the playback members written by C_animations_machine_de_luxe::Init.
/// Other animation-machine members are not reset or given guessed defaults.
#[derive(Clone, Debug, PartialEq)]
pub struct HumanAnimationPlayback {
    pub anim0_on: bool,
    pub anim0_stop_blended: bool,
    pub anim0_once: bool,
    pub anim1_on: bool,
    pub anim1_stop_blended: bool,
    pub anim1_once: bool,
    pub stage_blending: bool,
    pub blend: f32,
    pub blend_speed: f32,
    pub anim_index: u32,
    pub next_start: f32,
    pub track: i32,
    pub prev_track: i32,
    pub ext_on: bool,
    pub disabled: bool,
    pub unknown_10e: u16,
    pub anim1_id: u32,
    pub anim0_id: u32,
}

impl HumanAnimationPlayback {
    fn initialized() -> Self {
        Self {
            anim0_on: false,
            anim0_stop_blended: false,
            anim0_once: false,
            anim1_on: false,
            anim1_stop_blended: false,
            anim1_once: false,
            stage_blending: false,
            blend: 0.0,
            blend_speed: 0.0,
            anim_index: 0,
            next_start: -1.0,
            track: -1,
            prev_track: -1,
            ext_on: false,
            disabled: false,
            unknown_10e: u16::MAX,
            anim1_id: u32::MAX,
            anim0_id: u32::MAX,
        }
    }
}

/// Additional permanent Human members, not a second Human snapshot. Existing
/// car/motion/combat/step members remain in HumanState and are written there.
/// Scalar None and Unestablished mean no value has yet been established; an
/// Established(None) resource is an actual native null. Times are milliseconds,
/// positions use actor world units, and all floats retain their original bits.
#[derive(Debug, Default)]
pub struct HumanGameInitFields {
    pub orig_model: HumanGameInitValue<Option<Box<[u8]>>>,
    pub anim_set: Option<i32>,
    pub last_velocity: Option<[f32; 3]>,
    pub fall: Option<f32>,
    pub velocity_ramp: Option<f32>,
    pub idle_time: Option<i32>,
    pub breath_time: Option<i32>,
    pub striking: Option<bool>,
    pub dead_time: Option<i32>,
    pub last_fire_time: Option<i32>,
    pub track_test: Option<i32>,
    pub bleed_count: Option<i32>,
    pub traffic: HumanNullReference,
    pub human_flags: Option<u32>,
    pub move_time: Option<i16>,
    pub move_pos: Option<[f32; 3]>,
    pub move_ratio: Option<f32>,
    pub step_time: Option<u32>,
    pub move_coll: HumanNullReference,
    pub last_hit_time: Option<u32>,
    pub last_hit_render: Option<i32>,
    pub dead_body_human: HumanGameInitValue<Option<ActorReferenceId>>,
    pub dead_body_time: Option<i32>,
    pub lower_coll_off: Option<bool>,
    pub death_pos_set: Option<bool>,
    pub stealing_car: HumanGameInitValue<Option<ActorReferenceId>>,
    pub freeze_time: Option<i32>,
    pub effect_step: Option<i32>,
    pub pump_time: Option<i32>,
    pub hand_clip: HumanGameInitValue<Option<FrameReferenceId>>,
    pub killer: HumanGameInitValue<Option<ActorReferenceId>>,
    pub scream_areas: Option<u16>,
    pub no_fuel_car: HumanGameInitValue<Option<ActorReferenceId>>,
    pub next_car_action: Option<i32>,
    pub car_action_time: Option<i32>,
    pub temp_driver: Option<bool>,
    pub last_car: HumanGameInitValue<Option<ActorReferenceId>>,
    pub ab_created: Option<bool>,
    pub strike_time: Option<i32>,
    pub locked_car: HumanGameInitValue<Option<ActorReferenceId>>,
    pub locked_vehicle: HumanGameInitValue<Option<ActorReferenceId>>,
    pub shots: Option<f32>,
    pub hits: Option<f32>,
    pub prev_shots: Option<f32>,
    pub prev_hits: Option<f32>,
    pub ratio_time: Option<i32>,
    pub car_stance: Option<i32>,
    pub car_stance_time: Option<i32>,
    pub car_anim_index: Option<i32>,
    pub car_anim_length: Option<f32>,
    pub car_aim: Option<f32>,
    pub pose_moving: Option<bool>,
    pub pose_back1: HumanGameInitValue<Option<PoseHandle>>,
    pub pose_back2: HumanGameInitValue<Option<PoseHandle>>,
    pub pose_neck: HumanGameInitValue<Option<PoseHandle>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HumanGameInitBoundary {
    CreateDynColls,
    InsertLowerDynamicCollision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HumanGameInitStage {
    Enter,
    BaseActor,
    FindBaseFrame,
    RetainFrame,
    DeleteSceneFrame,
    DebustizeFrame,
    NoShadowFrame,
    AnimationInit,
    ResetMotion,
    ReadPosition,
    ResetGameplay,
    CreateBack1Pose,
    SetBack1Pose,
    CreateBack2Pose,
    SetBack2Pose,
    CreateNeckPose,
    SetNeckPose,
    FinishPoseSetup,
    EnterDynColls,
    AllocateCollisionSlots,
    ClearCollisionSlots,
    AllocateCollisionFrameSlots,
    SetLowerCollisionFrame,
    AllocateLowerDynamicCollision,
    SetLowerDynamicCollision,
    ClearLowerDynamicCollision,
    SetLowerCollisionKind,
    SetLowerCollisionFlags,
    ReadLowerWorldPosition,
    SetLowerCollisionSphere,
    SetLowerCollisionRadius,
    SetLowerDynamicCollisionFrame,
    SetLowerDynamicCollisionOwner,
    FinishLowerCollisionSetup,
}

/// Started also describes an interrupted attempt. Re-entry is refused whether
/// an effect failed, panicked, or reached the boundary; GameDone/re-entry needs
/// a separately evidenced lifecycle, not automatic destructive replay.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HumanGameInitStatus {
    #[default]
    NotStarted,
    Started(HumanGameInitStage),
    AtBoundary(HumanGameInitBoundary),
}

#[derive(Debug, Eq, PartialEq)]
pub enum HumanGameInitError<Error> {
    Backend {
        stage: HumanGameInitStage,
        source: Error,
    },
    MissingFrame {
        stage: HumanGameInitStage,
    },
    AlreadyAttempted {
        status: HumanGameInitStatus,
    },
    UnexpectedBoundary {
        expected: HumanGameInitBoundary,
        status: HumanGameInitStatus,
    },
    CollisionStorage {
        stage: HumanGameInitStage,
        source: HumanDynCollsStorageError,
    },
}

/// All methods are mandatory live dependencies, never successful markers.
/// Resolve actor/frame generations in their actual owners. `human[_mut]` and
/// actor reads borrow those owners without executing gameplay callbacks.
/// Find searches ENUMF_ALL (0xffff); retain wraps the native 32-bit refcount.
/// Delete resolves the live mission/scene at that call. Debustize/NoShadow
/// retain their null acceptance and reverse-child traversal. CreatePose uses
/// the actual driver on every call; SetPose receives even a null pose. Rc
/// sharing here does not replace the pose's native explicit reference count.
pub trait HumanGameInitBackend {
    type Error;

    fn human(&self, actor: ActorReferenceId) -> Result<&HumanState, Self::Error>;
    fn human_mut(&mut self, actor: ActorReferenceId) -> Result<&mut HumanState, Self::Error>;
    fn actor_frame(&self, actor: ActorReferenceId)
        -> Result<Option<FrameReferenceId>, Self::Error>;
    fn actor_position(&self, actor: ActorReferenceId) -> Result<[f32; 3], Self::Error>;
    fn base_actor_game_init(&mut self, actor: ActorReferenceId) -> Result<(), Self::Error>;
    fn find_child_frame_all(
        &mut self,
        frame: FrameReferenceId,
        name: &[u8],
    ) -> Result<Option<FrameReferenceId>, Self::Error>;
    fn retain_frame(&mut self, frame: FrameReferenceId) -> Result<(), Self::Error>;
    fn delete_scene_frame(
        &mut self,
        actor: ActorReferenceId,
        frame: Option<FrameReferenceId>,
    ) -> Result<(), Self::Error>;
    fn debustize_frame(&mut self, frame: Option<FrameReferenceId>) -> Result<(), Self::Error>;
    fn no_shadow_frame(&mut self, frame: Option<FrameReferenceId>) -> Result<(), Self::Error>;
    fn create_pose(&mut self) -> Result<Option<PoseHandle>, Self::Error>;
    fn set_model_pose(
        &mut self,
        frame: FrameReferenceId,
        name: &[u8],
        pose: Option<PoseHandle>,
    ) -> Result<(), Self::Error>;
}

/// Mandatory live allocation and frame-owner dependencies. Counts describe
/// native pointer slots (four bytes each), not host pointer sizes. The first
/// allocation always requests two; subsequent counts preserve the native
/// wrapping 32-bit byte shift. Allocation NULLs must be returned as None.
/// Returned arrays are the actual storage subsequently owned by HumanState.
/// Collider allocation includes the native zeroing constructor. World position
/// resolves the original cached frame identity, refreshing its world cache only
/// when dirty through the actual frame graph. Never substitute actor_position.
/// No borrow of collision storage is held across any of these callbacks.
pub trait HumanDynCollsSetupBackend: HumanGameInitBackend {
    fn frame_world_position(&mut self, frame: FrameReferenceId) -> Result<[f32; 3], Self::Error>;
    fn allocate_collision_slots(
        &mut self,
        count: u32,
    ) -> Result<Option<HumanDynamicCollisionSlots>, Self::Error>;
    fn allocate_collision_frame_slots(
        &mut self,
        count: u32,
    ) -> Result<Option<HumanCollisionFrameSlots>, Self::Error>;
    fn allocate_zeroed_dynamic_collision(
        &mut self,
    ) -> Result<Option<HumanDynamicCollisionHandle>, Self::Error>;
}

fn backend_error<Error>(stage: HumanGameInitStage, source: Error) -> HumanGameInitError<Error> {
    HumanGameInitError::Backend { stage, source }
}

fn stage_human<Backend: HumanGameInitBackend>(
    backend: &mut Backend,
    actor: ActorReferenceId,
    stage: HumanGameInitStage,
) -> Result<&mut HumanState, HumanGameInitError<Backend::Error>> {
    let human = backend
        .human_mut(actor)
        .map_err(|source| backend_error(stage, source))?;
    human.game_init_status = HumanGameInitStatus::Started(stage);
    Ok(human)
}

fn actor_frame<Backend: HumanGameInitBackend>(
    backend: &Backend,
    actor: ActorReferenceId,
    stage: HumanGameInitStage,
) -> Result<Option<FrameReferenceId>, HumanGameInitError<Backend::Error>> {
    backend
        .actor_frame(actor)
        .map_err(|source| backend_error(stage, source))
}

/// Runs the inherited Human prefix only. No collision/sound/shadow/inventory
/// suffix or Player GameInit post-call stores execute at the returned boundary.
pub fn game_init_through_pose_setup<Backend: HumanGameInitBackend>(
    backend: &mut Backend,
    actor: ActorReferenceId,
) -> Result<HumanGameInitBoundary, HumanGameInitError<Backend::Error>> {
    use HumanGameInitStage as Stage;
    use HumanGameInitValue::Established;

    let human = backend
        .human_mut(actor)
        .map_err(|source| backend_error(Stage::Enter, source))?;
    if human.game_init_status != HumanGameInitStatus::NotStarted {
        return Err(HumanGameInitError::AlreadyAttempted {
            status: human.game_init_status,
        });
    }
    human.game_init_status = HumanGameInitStatus::Started(Stage::BaseActor);
    human.game_init_fields.orig_model = Established(None);
    backend
        .base_actor_game_init(actor)
        .map_err(|source| backend_error(Stage::BaseActor, source))?;

    stage_human(backend, actor, Stage::FindBaseFrame)?;
    let frame = actor_frame(backend, actor, Stage::FindBaseFrame)?.ok_or(
        HumanGameInitError::MissingFrame {
            stage: Stage::FindBaseFrame,
        },
    )?;
    let base = backend
        .find_child_frame_all(frame, b"base")
        .map_err(|source| backend_error(Stage::FindBaseFrame, source))?;
    stage_human(backend, actor, Stage::RetainFrame)?.base_frame = base;
    if let Some(frame) = actor_frame(backend, actor, Stage::RetainFrame)? {
        backend
            .retain_frame(frame)
            .map_err(|source| backend_error(Stage::RetainFrame, source))?;
        stage_human(backend, actor, Stage::DeleteSceneFrame)?;
        let frame = actor_frame(backend, actor, Stage::DeleteSceneFrame)?;
        backend
            .delete_scene_frame(actor, frame)
            .map_err(|source| backend_error(Stage::DeleteSceneFrame, source))?;
    }
    stage_human(backend, actor, Stage::DebustizeFrame)?;
    let frame = actor_frame(backend, actor, Stage::DebustizeFrame)?;
    backend
        .debustize_frame(frame)
        .map_err(|source| backend_error(Stage::DebustizeFrame, source))?;
    stage_human(backend, actor, Stage::NoShadowFrame)?;
    let frame = actor_frame(backend, actor, Stage::NoShadowFrame)?;
    backend
        .no_shadow_frame(frame)
        .map_err(|source| backend_error(Stage::NoShadowFrame, source))?;

    let human = stage_human(backend, actor, Stage::AnimationInit)?;
    human.car = None;
    human.using_car = None;
    human.action_state = HUMAN_ACTION_STAND;
    human.vehicle_action = 0;
    human.animation_machine.owner = Some(actor);
    human.animation_machine.untouched_word = 0;
    let model = actor_frame(backend, actor, Stage::AnimationInit)?;
    let human = stage_human(backend, actor, Stage::ResetMotion)?;
    human.animation_machine.model = model;
    human.animation_machine.playback = Some(HumanAnimationPlayback::initialized());
    human.next_anim_id = ANIM_BREATH01A;
    human.current_properties = human.properties.clone();
    human.anim_id = 0;
    human.game_init_fields.anim_set = Some(0);
    human.velocity = [0.0; 3];
    human.game_init_fields.last_velocity = Some([0.0; 3]);
    human.turn_speed = 0.0;
    human.game_init_fields.fall = Some(0.0);
    human.game_init_fields.velocity_ramp = Some(0.0);
    human.guarding = false;

    stage_human(backend, actor, Stage::ReadPosition)?;
    let position = backend
        .actor_position(actor)
        .map_err(|source| backend_error(Stage::ReadPosition, source))?;
    let human = stage_human(backend, actor, Stage::ResetGameplay)?;
    human.game_init_fields.idle_time = Some(-1);
    human.game_init_fields.breath_time = Some(-1);
    human.game_init_fields.striking = Some(false);
    human.sniper_mode = false;
    human.game_init_fields.dead_time = Some(500);
    human.agony_anim = -1;
    human.wingman = -1;
    human.game_init_fields.last_fire_time = Some(0);
    human.fire_target = None;
    human.game_init_fields.track_test = Some(0);
    human.game_init_fields.bleed_count = Some(0);
    human.track_steps = 0;
    human.game_init_fields.traffic = HumanNullReference::Null;
    human.carried_box = HumanCarriedFrame::Absent;
    human.game_init_fields.human_flags = Some(0);
    human.game_init_fields.move_time = Some(0);
    human.game_init_fields.move_pos = Some(position);
    human.game_init_fields.move_ratio = Some(1.0);
    human.game_init_fields.step_time = Some(0);
    human.game_init_fields.move_coll = HumanNullReference::Null;
    human.no_anim_hit = false;
    human.note_fire = false;
    human.reloading = false;
    human.reload_forced = false;
    human.loading_shells = false;
    human.game_init_fields.last_hit_time = Some(0);
    human.game_init_fields.last_hit_render = Some(0);
    human.idle_anims = false;
    human.test_ground = true;
    human.voice_priority = -1;
    human.carried_dead_body = HumanCarriedFrame::Absent;
    human.game_init_fields.dead_body_human = Established(None);
    human.game_init_fields.dead_body_time = Some(-1);
    human.game_init_fields.lower_coll_off = Some(false);
    human.killed_by_shot = false;
    human.game_init_fields.death_pos_set = Some(false);
    human.game_init_fields.stealing_car = Established(None);
    human.game_init_fields.freeze_time = Some(-1);
    human.game_init_fields.effect_step = Some(0);
    human.game_init_fields.pump_time = Some(-1);
    human.game_init_fields.hand_clip = Established(None);
    human.game_init_fields.killer = Established(None);
    human.drowned = false;
    human.changing_weapon = false;
    human.game_init_fields.scream_areas = Some(1);
    human.game_init_fields.no_fuel_car = Established(None);
    human.game_init_fields.next_car_action = Some(0);
    human.game_init_fields.car_action_time = Some(-1);
    human.speed_limiter = false;
    human.strike_anim_time = -1;
    human.score_bonus = false;
    human.game_init_fields.temp_driver = Some(false);
    human.game_init_fields.last_car = Established(None);
    human.weapon_displaced = false;
    human.reload_effects = false;
    human.shut_up = false;
    human.game_init_fields.ab_created = Some(true);
    human.game_init_fields.strike_time = Some(0);
    human.game_init_fields.locked_car = Established(None);
    human.game_init_fields.locked_vehicle = Established(None);
    human.game_init_fields.shots = Some(1.0);
    human.game_init_fields.hits = Some(1.0);
    human.game_init_fields.prev_shots = Some(1.0);
    human.game_init_fields.prev_hits = Some(1.0);
    human.game_init_fields.ratio_time = Some(5000);
    human.game_init_fields.car_stance = Some(0);
    human.game_init_fields.car_stance_time = Some(0);
    human.game_init_fields.car_anim_index = Some(0);
    human.game_init_fields.car_anim_length = Some(0.0);
    human.game_init_fields.car_aim = Some(0.5);
    human.game_init_fields.pose_moving = Some(false);

    for (create_stage, set_stage, name) in [
        (
            Stage::CreateBack1Pose,
            Stage::SetBack1Pose,
            b"back1".as_slice(),
        ),
        (
            Stage::CreateBack2Pose,
            Stage::SetBack2Pose,
            b"back2".as_slice(),
        ),
        (
            Stage::CreateNeckPose,
            Stage::SetNeckPose,
            b"neck".as_slice(),
        ),
    ] {
        stage_human(backend, actor, create_stage)?;
        let pose = backend
            .create_pose()
            .map_err(|source| backend_error(create_stage, source))?;
        let frame = actor_frame(backend, actor, set_stage)?;
        let human = stage_human(backend, actor, set_stage)?;
        match create_stage {
            Stage::CreateBack1Pose => human.game_init_fields.pose_back1 = Established(pose.clone()),
            Stage::CreateBack2Pose => human.game_init_fields.pose_back2 = Established(pose.clone()),
            Stage::CreateNeckPose => human.game_init_fields.pose_neck = Established(pose.clone()),
            _ => unreachable!(),
        }
        let frame = frame.ok_or(HumanGameInitError::MissingFrame { stage: set_stage })?;
        backend
            .set_model_pose(frame, name, pose)
            .map_err(|source| backend_error(set_stage, source))?;
    }
    let human = stage_human(backend, actor, Stage::FinishPoseSetup)?;
    human.look_frame = None;
    human.anim_step = 0;
    human.anim_steps[0].step_type = HUMAN_STEP_END;
    human.game_init_status = HumanGameInitStatus::AtBoundary(HumanGameInitBoundary::CreateDynColls);
    Ok(HumanGameInitBoundary::CreateDynColls)
}

fn collision_error<Error>(
    stage: HumanGameInitStage,
    source: HumanDynCollsStorageError,
) -> HumanGameInitError<Error> {
    HumanGameInitError::CollisionStorage { stage, source }
}

fn collision_slot_count(count: &HumanGameInitValue<u32>) -> Result<u32, HumanDynCollsStorageError> {
    match count {
        HumanGameInitValue::Established(count) => Ok(count.wrapping_shl(2) >> 2),
        HumanGameInitValue::Unestablished => Err(HumanDynCollsStorageError::UnestablishedCount),
    }
}

fn collision_slots_mut(
    human: &mut HumanState,
) -> Result<&mut HumanDynamicCollisionSlots, HumanDynCollsStorageError> {
    match &mut human.dynamic_collisions.colls {
        HumanGameInitValue::Established(Some(slots)) => Ok(slots),
        HumanGameInitValue::Established(None) => Err(HumanDynCollsStorageError::NullCollisionSlots),
        HumanGameInitValue::Unestablished => {
            Err(HumanDynCollsStorageError::UnestablishedCollisionSlots)
        }
    }
}

fn lower_collision(
    human: &HumanState,
) -> Result<&HumanDynamicCollisionHandle, HumanDynCollsStorageError> {
    let slots = match &human.dynamic_collisions.colls {
        HumanGameInitValue::Established(Some(slots)) => slots,
        HumanGameInitValue::Established(None) => {
            return Err(HumanDynCollsStorageError::NullCollisionSlots);
        }
        HumanGameInitValue::Unestablished => {
            return Err(HumanDynCollsStorageError::UnestablishedCollisionSlots);
        }
    };
    match slots.slots.first() {
        Some(HumanGameInitValue::Established(Some(collision))) => Ok(collision),
        Some(HumanGameInitValue::Established(None)) => {
            Err(HumanDynCollsStorageError::NullCollision { index: 0 })
        }
        Some(HumanGameInitValue::Unestablished) => {
            Err(HumanDynCollsStorageError::UnestablishedCollision { index: 0 })
        }
        None => Err(HumanDynCollsStorageError::CollisionSlotOutOfBounds { index: 0, len: 0 }),
    }
}

fn write_lower_collision<Backend: HumanGameInitBackend>(
    backend: &mut Backend,
    actor: ActorReferenceId,
    stage: HumanGameInitStage,
    write: impl FnOnce(&mut HumanDynamicCollision),
) -> Result<(), HumanGameInitError<Backend::Error>> {
    let human = stage_human(backend, actor, stage)?;
    let collision = lower_collision(human).map_err(|source| collision_error(stage, source))?;
    let mut collision = collision.try_borrow_mut().map_err(|_| {
        collision_error(
            stage,
            HumanDynCollsStorageError::CollisionBorrowConflict { index: 0 },
        )
    })?;
    write(&mut collision);
    Ok(())
}

fn lower_collision_frame(
    human: &HumanState,
) -> Result<Option<FrameReferenceId>, HumanDynCollsStorageError> {
    let slots = match &human.dynamic_collisions.coll_frames {
        HumanGameInitValue::Established(Some(slots)) => slots,
        HumanGameInitValue::Established(None) => {
            return Err(HumanDynCollsStorageError::NullFrameSlots);
        }
        HumanGameInitValue::Unestablished => {
            return Err(HumanDynCollsStorageError::UnestablishedFrameSlots);
        }
    };
    match slots.slots.first() {
        Some(HumanGameInitValue::Established(frame)) => Ok(*frame),
        Some(HumanGameInitValue::Unestablished) => {
            Err(HumanDynCollsStorageError::UnestablishedFrame { index: 0 })
        }
        None => Err(HumanDynCollsStorageError::FrameSlotOutOfBounds { index: 0, len: 0 }),
    }
}

/// Resumes only the accepted CreateDynColls boundary; never replays pose setup.
/// Ports RVA 0x175ca0 through lower setup, stopping before DynInsert (0x175db1).
/// Allocation stores and reached NULL/bounds failures preserve the native effect
/// prefix. World-position callbacks may redirect the live arrays/collider/frame;
/// the source frame stays cached while subsequent destinations are read afresh.
/// No manager insertion, upper setup, erasure, colls_active write or later Human
/// and Player suffix executes. DynInsert still needs a typed live manager bridge
/// and its own numeric contract; successful setup is not successful GameInit.
pub fn game_init_through_lower_collision_setup<Backend: HumanDynCollsSetupBackend>(
    backend: &mut Backend,
    actor: ActorReferenceId,
) -> Result<HumanGameInitBoundary, HumanGameInitError<Backend::Error>> {
    use HumanGameInitStage as Stage;
    use HumanGameInitValue::Established;

    let human = backend
        .human_mut(actor)
        .map_err(|source| backend_error(Stage::EnterDynColls, source))?;
    let expected = HumanGameInitBoundary::CreateDynColls;
    if human.game_init_status != HumanGameInitStatus::AtBoundary(expected) {
        return Err(HumanGameInitError::UnexpectedBoundary {
            expected,
            status: human.game_init_status,
        });
    }
    human.game_init_status = HumanGameInitStatus::Started(Stage::AllocateCollisionSlots);
    human.dynamic_collisions.num_colls = Established(2);
    let slots = backend
        .allocate_collision_slots(2)
        .map_err(|source| backend_error(Stage::AllocateCollisionSlots, source))?;
    let human = stage_human(backend, actor, Stage::ClearCollisionSlots)?;
    let count = collision_slot_count(&human.dynamic_collisions.num_colls)
        .map_err(|source| collision_error(Stage::ClearCollisionSlots, source))?;
    human.dynamic_collisions.colls = Established(slots);
    if count != 0 {
        let slots = collision_slots_mut(human)
            .map_err(|source| collision_error(Stage::ClearCollisionSlots, source))?;
        for slot in slots.slots.iter_mut().take(count as usize) {
            *slot = Established(None);
        }
        if count as usize > slots.slots.len() {
            return Err(collision_error(
                Stage::ClearCollisionSlots,
                HumanDynCollsStorageError::CollisionSlotOutOfBounds {
                    index: slots.slots.len() as u32,
                    len: slots.slots.len(),
                },
            ));
        }
    }

    let human = stage_human(backend, actor, Stage::AllocateCollisionFrameSlots)?;
    let count = collision_slot_count(&human.dynamic_collisions.num_colls)
        .map_err(|source| collision_error(Stage::AllocateCollisionFrameSlots, source))?;
    let frames = backend
        .allocate_collision_frame_slots(count)
        .map_err(|source| backend_error(Stage::AllocateCollisionFrameSlots, source))?;
    stage_human(backend, actor, Stage::SetLowerCollisionFrame)?;
    let frame = actor_frame(backend, actor, Stage::SetLowerCollisionFrame)?;
    let human = stage_human(backend, actor, Stage::SetLowerCollisionFrame)?;
    human.dynamic_collisions.coll_frames = Established(frames);
    let frames = match &mut human.dynamic_collisions.coll_frames {
        Established(Some(frames)) => frames,
        Established(None) => {
            return Err(collision_error(
                Stage::SetLowerCollisionFrame,
                HumanDynCollsStorageError::NullFrameSlots,
            ));
        }
        HumanGameInitValue::Unestablished => unreachable!(),
    };
    let frame_len = frames.slots.len();
    *frames.slots.first_mut().ok_or_else(|| {
        collision_error(
            Stage::SetLowerCollisionFrame,
            HumanDynCollsStorageError::FrameSlotOutOfBounds {
                index: 0,
                len: frame_len,
            },
        )
    })? = Established(frame);

    stage_human(backend, actor, Stage::AllocateLowerDynamicCollision)?;
    let collision = backend
        .allocate_zeroed_dynamic_collision()
        .map_err(|source| backend_error(Stage::AllocateLowerDynamicCollision, source))?;
    let human = stage_human(backend, actor, Stage::SetLowerDynamicCollision)?;
    let slots = collision_slots_mut(human)
        .map_err(|source| collision_error(Stage::SetLowerDynamicCollision, source))?;
    let collision_len = slots.slots.len();
    *slots.slots.first_mut().ok_or_else(|| {
        collision_error(
            Stage::SetLowerDynamicCollision,
            HumanDynCollsStorageError::CollisionSlotOutOfBounds {
                index: 0,
                len: collision_len,
            },
        )
    })? = Established(collision);
    write_lower_collision(
        backend,
        actor,
        Stage::ClearLowerDynamicCollision,
        |collision| {
            *collision = HumanDynamicCollision::zeroed();
        },
    )?;
    write_lower_collision(backend, actor, Stage::SetLowerCollisionKind, |collision| {
        collision.kind = 0x42;
    })?;
    write_lower_collision(backend, actor, Stage::SetLowerCollisionFlags, |collision| {
        collision.flags = 0x101;
    })?;

    let human = stage_human(backend, actor, Stage::ReadLowerWorldPosition)?;
    let frame = lower_collision_frame(human)
        .map_err(|source| collision_error(Stage::ReadLowerWorldPosition, source))?
        .ok_or(HumanGameInitError::MissingFrame {
            stage: Stage::ReadLowerWorldPosition,
        })?;
    let position = backend
        .frame_world_position(frame)
        .map_err(|source| backend_error(Stage::ReadLowerWorldPosition, source))?;
    write_lower_collision(
        backend,
        actor,
        Stage::SetLowerCollisionSphere,
        |collision| {
            collision.sphere_position = position;
        },
    )?;
    write_lower_collision(
        backend,
        actor,
        Stage::SetLowerCollisionRadius,
        |collision| {
            collision.sphere_radius = f32::from_bits(0x3e99_999a);
        },
    )?;
    stage_human(backend, actor, Stage::SetLowerDynamicCollisionFrame)?;
    let frame = actor_frame(backend, actor, Stage::SetLowerDynamicCollisionFrame)?;
    write_lower_collision(
        backend,
        actor,
        Stage::SetLowerDynamicCollisionFrame,
        |collision| {
            collision.frame = frame;
        },
    )?;
    write_lower_collision(
        backend,
        actor,
        Stage::SetLowerDynamicCollisionOwner,
        |collision| {
            collision.owner = Some(actor);
        },
    )?;
    stage_human(backend, actor, Stage::FinishLowerCollisionSetup)?.game_init_status =
        HumanGameInitStatus::AtBoundary(HumanGameInitBoundary::InsertLowerDynamicCollision);
    Ok(HumanGameInitBoundary::InsertLowerDynamicCollision)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation_pose::I3dPose;
    use crate::human_actor::{
        HumanAnimationMachineSeed, HumanAuxiliarySeed, HumanProperties, HumanSeed,
    };
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Event {
        Base,
        Frame,
        Find(FrameReferenceId, Vec<u8>),
        Retain(FrameReferenceId),
        Delete(Option<FrameReferenceId>),
        Debustize(Option<FrameReferenceId>),
        NoShadow(Option<FrameReferenceId>),
        Position,
        Create(usize),
        Set(FrameReferenceId, Vec<u8>, bool),
        AllocateCollisions(u32),
        AllocateCollisionFrames(u32),
        AllocateLowerCollision,
        WorldPosition(FrameReferenceId),
    }

    #[derive(Default)]
    struct CollisionCallbacks {
        null_slots: bool,
        null_frames: bool,
        null_collision: bool,
        on_slots: Option<fn(&mut Backend)>,
        on_frames: Option<fn(&mut Backend)>,
        on_collision: Option<fn(&mut Backend)>,
        on_world: Option<fn(&mut Backend)>,
        allocated_collision: Option<HumanDynamicCollisionHandle>,
    }

    struct Backend {
        actor: ActorReferenceId,
        human: HumanState,
        frame: Option<FrameReferenceId>,
        base: Option<FrameReferenceId>,
        position: [f32; 3],
        events: RefCell<Vec<Event>>,
        fail: Option<HumanGameInitStage>,
        fail_read: Option<HumanGameInitStage>,
        redirect: bool,
        null_after_find: bool,
        null_after_retain: bool,
        null_in_create: Option<usize>,
        null_pose: Option<usize>,
        panic_base: bool,
        poses: Vec<Option<PoseHandle>>,
        collision: CollisionCallbacks,
    }

    fn frame(identity: u32) -> FrameReferenceId {
        FrameReferenceId::from_existing(identity)
    }

    fn seed() -> HumanSeed {
        HumanSeed {
            allocator_tag: 0x97,
            animation_machine: HumanAnimationMachineSeed {
                untouched_word: 0xfeed_abcdu32,
            },
            previous_animation_code: 0x1234_5678,
            requested_animation_code: 0x8765_4321,
            human_type_mask: 0x1122_3344,
            current_properties: HumanProperties::from_record([0xda; 64]),
            target_e: Some(frame(800)),
            target_n: Some(frame(801)),
            base_frame: Some(frame(802)),
            auxiliary: std::array::from_fn(|_| HumanAuxiliarySeed {
                untouched_word: 0x1122_3344,
                padding: [0xa7; 3],
                untouched_tail: [0x5d; 16],
            }),
        }
    }

    impl Backend {
        fn new() -> Self {
            Self {
                actor: ActorReferenceId::from_existing(19),
                human: HumanState::construct(seed()),
                frame: Some(frame(1)),
                base: Some(frame(900)),
                position: [
                    f32::from_bits(0x7fc1_2345),
                    -0.0,
                    f32::from_bits(0xff80_0000),
                ],
                events: RefCell::new(Vec::new()),
                fail: None,
                fail_read: None,
                redirect: false,
                null_after_find: false,
                null_after_retain: false,
                null_in_create: None,
                null_pose: None,
                panic_base: false,
                poses: Vec::new(),
                collision: CollisionCallbacks::default(),
            }
        }

        fn check(&self, stage: HumanGameInitStage) -> Result<(), HumanGameInitStage> {
            if self.fail == Some(stage) {
                Err(stage)
            } else {
                Ok(())
            }
        }

        fn current_stage(&self) -> HumanGameInitStage {
            match self.human.game_init_status() {
                HumanGameInitStatus::Started(stage) => stage,
                _ => HumanGameInitStage::Enter,
            }
        }

        fn run(&mut self) -> Result<HumanGameInitBoundary, HumanGameInitError<HumanGameInitStage>> {
            game_init_through_pose_setup(self, self.actor)
        }

        fn at_collision_boundary() -> Self {
            let mut backend = Self::new();
            backend.run().unwrap();
            backend.events.borrow_mut().clear();
            backend
        }

        fn run_collision(
            &mut self,
        ) -> Result<HumanGameInitBoundary, HumanGameInitError<HumanGameInitStage>> {
            game_init_through_lower_collision_setup(self, self.actor)
        }

        fn lower_collision(&self) -> HumanDynamicCollisionHandle {
            lower_collision(&self.human).unwrap().clone()
        }
    }

    impl HumanGameInitBackend for Backend {
        type Error = HumanGameInitStage;

        fn human(&self, actor: ActorReferenceId) -> Result<&HumanState, Self::Error> {
            if actor == self.actor {
                Ok(&self.human)
            } else {
                Err(HumanGameInitStage::Enter)
            }
        }

        fn human_mut(&mut self, actor: ActorReferenceId) -> Result<&mut HumanState, Self::Error> {
            if actor == self.actor {
                Ok(&mut self.human)
            } else {
                Err(HumanGameInitStage::Enter)
            }
        }

        fn actor_frame(
            &self,
            actor: ActorReferenceId,
        ) -> Result<Option<FrameReferenceId>, Self::Error> {
            self.human(actor)?;
            self.events.borrow_mut().push(Event::Frame);
            let stage = self.current_stage();
            if self.fail_read == Some(stage) {
                Err(stage)
            } else {
                Ok(self.frame)
            }
        }

        fn actor_position(&self, actor: ActorReferenceId) -> Result<[f32; 3], Self::Error> {
            self.human(actor)?;
            self.events.borrow_mut().push(Event::Position);
            if self.fail_read == Some(HumanGameInitStage::ReadPosition) {
                Err(HumanGameInitStage::ReadPosition)
            } else {
                Ok(self.position)
            }
        }

        fn base_actor_game_init(&mut self, actor: ActorReferenceId) -> Result<(), Self::Error> {
            assert_eq!(actor, self.actor);
            assert!(matches!(
                self.human.game_init_fields.orig_model,
                HumanGameInitValue::Established(None)
            ));
            self.events.borrow_mut().push(Event::Base);
            self.human.alive = true;
            if self.panic_base {
                panic!("synthetic interrupted base dependency");
            }
            if self.redirect {
                self.frame = Some(frame(2));
            }
            self.check(HumanGameInitStage::BaseActor)
        }

        fn find_child_frame_all(
            &mut self,
            parent: FrameReferenceId,
            name: &[u8],
        ) -> Result<Option<FrameReferenceId>, Self::Error> {
            self.events
                .borrow_mut()
                .push(Event::Find(parent, name.to_vec()));
            if self.redirect {
                self.frame = Some(frame(3));
            }
            if self.null_after_find {
                self.frame = None;
            }
            self.check(HumanGameInitStage::FindBaseFrame)?;
            Ok(self.base)
        }

        fn retain_frame(&mut self, retained: FrameReferenceId) -> Result<(), Self::Error> {
            self.events.borrow_mut().push(Event::Retain(retained));
            if self.null_after_retain {
                self.frame = None;
            }
            self.check(HumanGameInitStage::RetainFrame)
        }

        fn delete_scene_frame(
            &mut self,
            actor: ActorReferenceId,
            deleted: Option<FrameReferenceId>,
        ) -> Result<(), Self::Error> {
            assert_eq!(actor, self.actor);
            self.events.borrow_mut().push(Event::Delete(deleted));
            if self.redirect {
                self.frame = Some(frame(4));
            }
            self.check(HumanGameInitStage::DeleteSceneFrame)
        }

        fn debustize_frame(
            &mut self,
            debustized: Option<FrameReferenceId>,
        ) -> Result<(), Self::Error> {
            self.events.borrow_mut().push(Event::Debustize(debustized));
            if self.redirect {
                self.frame = Some(frame(5));
            }
            self.check(HumanGameInitStage::DebustizeFrame)
        }

        fn no_shadow_frame(
            &mut self,
            unshadowed: Option<FrameReferenceId>,
        ) -> Result<(), Self::Error> {
            self.events.borrow_mut().push(Event::NoShadow(unshadowed));
            if self.redirect {
                self.frame = Some(frame(6));
                self.human.properties =
                    HumanProperties::from_record(std::array::from_fn(|index| {
                        (index as u8).wrapping_mul(73)
                    }));
            }
            self.check(HumanGameInitStage::NoShadowFrame)
        }

        fn create_pose(&mut self) -> Result<Option<PoseHandle>, Self::Error> {
            let index = self.poses.len();
            self.events.borrow_mut().push(Event::Create(index));
            let stage = self.current_stage();
            assert_eq!(
                self.human.game_init_fields.car_aim.map(f32::to_bits),
                Some(0x3f00_0000)
            );
            if self.redirect {
                self.frame = Some(frame(10 + index as u32));
            }
            if self.null_in_create == Some(index) {
                self.frame = None;
            }
            self.check(stage)?;
            let pose = (self.null_pose != Some(index))
                .then(|| Rc::new(RefCell::new(I3dPose::new_in([0.0, 1.0, 0.0]))));
            self.poses.push(pose.clone());
            Ok(pose)
        }

        fn set_model_pose(
            &mut self,
            model: FrameReferenceId,
            name: &[u8],
            pose: Option<PoseHandle>,
        ) -> Result<(), Self::Error> {
            self.events
                .borrow_mut()
                .push(Event::Set(model, name.to_vec(), pose.is_some()));
            let stored = match name {
                b"back1" => &self.human.game_init_fields.pose_back1,
                b"back2" => &self.human.game_init_fields.pose_back2,
                b"neck" => &self.human.game_init_fields.pose_neck,
                _ => panic!("unexpected pose name"),
            };
            match (stored, &pose) {
                (HumanGameInitValue::Established(Some(stored)), Some(pose)) => {
                    assert!(Rc::ptr_eq(stored, pose))
                }
                (HumanGameInitValue::Established(None), None) => {}
                _ => panic!("SetPose before the corresponding live pose store"),
            }
            if self.redirect {
                self.frame = Some(frame(20 + self.poses.len() as u32));
            }
            self.check(self.current_stage())
        }
    }

    impl HumanDynCollsSetupBackend for Backend {
        fn frame_world_position(
            &mut self,
            source: FrameReferenceId,
        ) -> Result<[f32; 3], Self::Error> {
            self.events.borrow_mut().push(Event::WorldPosition(source));
            if let Some(callback) = self.collision.on_world {
                callback(self);
            }
            self.check(HumanGameInitStage::ReadLowerWorldPosition)?;
            Ok(self.position)
        }

        fn allocate_collision_slots(
            &mut self,
            count: u32,
        ) -> Result<Option<HumanDynamicCollisionSlots>, Self::Error> {
            self.events
                .borrow_mut()
                .push(Event::AllocateCollisions(count));
            assert_eq!(
                self.human.dynamic_collisions.num_colls,
                HumanGameInitValue::Established(2)
            );
            let allocated = (!self.collision.null_slots)
                .then(|| HumanDynamicCollisionSlots::try_allocate(count).unwrap());
            if let Some(callback) = self.collision.on_slots {
                callback(self);
            }
            self.check(HumanGameInitStage::AllocateCollisionSlots)?;
            Ok(allocated)
        }

        fn allocate_collision_frame_slots(
            &mut self,
            count: u32,
        ) -> Result<Option<HumanCollisionFrameSlots>, Self::Error> {
            self.events
                .borrow_mut()
                .push(Event::AllocateCollisionFrames(count));
            let allocated = (!self.collision.null_frames)
                .then(|| HumanCollisionFrameSlots::try_allocate(count).unwrap());
            if let Some(callback) = self.collision.on_frames {
                callback(self);
            }
            self.check(HumanGameInitStage::AllocateCollisionFrameSlots)?;
            Ok(allocated)
        }

        fn allocate_zeroed_dynamic_collision(
            &mut self,
        ) -> Result<Option<HumanDynamicCollisionHandle>, Self::Error> {
            self.events.borrow_mut().push(Event::AllocateLowerCollision);
            let allocated = if self.collision.null_collision {
                None
            } else {
                Some(
                    self.collision
                        .allocated_collision
                        .clone()
                        .unwrap_or_else(|| Rc::new(RefCell::new(HumanDynamicCollision::zeroed()))),
                )
            };
            self.collision.allocated_collision = allocated.clone();
            if let Some(callback) = self.collision.on_collision {
                callback(self);
            }
            self.check(HumanGameInitStage::AllocateLowerDynamicCollision)?;
            Ok(allocated)
        }
    }

    #[test]
    fn constructor_does_not_invent_game_init_values() {
        let backend = Backend::new();
        assert_eq!(
            backend.human.game_init_status(),
            HumanGameInitStatus::NotStarted
        );
        assert!(matches!(
            backend.human.game_init_fields.orig_model,
            HumanGameInitValue::Unestablished
        ));
        assert!(matches!(
            backend.human.game_init_fields.pose_neck,
            HumanGameInitValue::Unestablished
        ));
        assert_eq!(backend.human.game_init_fields.car_aim, None);
        assert_eq!(
            backend.human.dynamic_collisions.num_colls,
            HumanGameInitValue::Unestablished
        );
        assert!(matches!(
            backend.human.dynamic_collisions.colls,
            HumanGameInitValue::Unestablished
        ));
        assert!(matches!(
            backend.human.dynamic_collisions.coll_frames,
            HumanGameInitValue::Unestablished
        ));
        assert_eq!(
            backend.human.game_init_fields.traffic,
            HumanNullReference::Unestablished
        );
        assert_eq!(
            backend.human.game_init_fields.move_coll,
            HumanNullReference::Unestablished
        );
        assert!(backend.human.animation_machine().playback().is_none());
        assert_eq!(
            backend
                .human
                .animation_machine()
                .untouched_constructor_word(),
            0xfeed_abcd
        );
    }

    #[test]
    fn exact_dependency_order_and_fresh_frame_reads() {
        let mut backend = Backend::new();
        backend.redirect = true;
        assert_eq!(backend.run(), Ok(HumanGameInitBoundary::CreateDynColls));
        assert_eq!(
            *backend.events.borrow(),
            vec![
                Event::Base,
                Event::Frame,
                Event::Find(frame(2), b"base".to_vec()),
                Event::Frame,
                Event::Retain(frame(3)),
                Event::Frame,
                Event::Delete(Some(frame(3))),
                Event::Frame,
                Event::Debustize(Some(frame(4))),
                Event::Frame,
                Event::NoShadow(Some(frame(5))),
                Event::Frame,
                Event::Position,
                Event::Create(0),
                Event::Frame,
                Event::Set(frame(10), b"back1".to_vec(), true),
                Event::Create(1),
                Event::Frame,
                Event::Set(frame(11), b"back2".to_vec(), true),
                Event::Create(2),
                Event::Frame,
                Event::Set(frame(12), b"neck".to_vec(), true),
            ]
        );
        assert_eq!(backend.human.base_frame(), Some(frame(900)));
        assert_eq!(backend.human.animation_machine().model(), Some(frame(6)));
        assert_eq!(
            backend.human.animation_machine().owner(),
            Some(backend.actor)
        );
        assert_eq!(
            backend.human.current_properties(),
            backend.human.properties()
        );
        assert_ne!(backend.human.current_properties().record(), &[0xda; 64]);
    }

    #[test]
    fn exact_scalar_resets_bit_copies_and_untouched_members() {
        let mut backend = Backend::new();
        let human = &mut backend.human;
        human.seat = -29;
        human.car = Some(ActorReferenceId::from_existing(41));
        human.using_car = human.car;
        human.vehicle_action = 77;
        human.action_state = -123;
        human.anim_id = 456;
        human.next_anim_id = 789;
        human.velocity = [-0.0; 3];
        human.turn_speed = f32::NAN;
        human.guarding = true;
        human.crouching = true;
        human.aimed = true;
        human.fire_held = true;
        human.sniper_mode = true;
        human.agony_anim = 91;
        human.fire_target = Some(frame(92));
        human.no_anim_hit = true;
        human.note_fire = true;
        human.reloading = true;
        human.reload_forced = true;
        human.loading_shells = true;
        human.idle_anims = true;
        human.test_ground = false;
        human.voice_priority = 93;
        human.killed_by_shot = true;
        human.drowned = true;
        human.changing_weapon = true;
        human.speed_limiter = true;
        human.strike_anim_time = 94;
        human.score_bonus = true;
        human.weapon_displaced = true;
        human.reload_effects = true;
        human.shut_up = true;
        human.wingman = 7;
        human.wingman_name = 0xface_0123;
        human.track_steps = i16::MIN;
        human.track_steps_max = i16::MAX;
        human.track_texture = 0xff;
        human.agony_count = 98;
        human.dir = [-0.0, 9.0, -2.0];
        human.set_carried_box_frame(Some(frame(71)));
        human.set_carried_dead_body_frame(Some(frame(72)));
        human.look_frame = Some(frame(73));
        human.anim_steps = std::array::from_fn(|index| super::super::HumanAnimStep {
            step_type: 21 + index as i32,
            anim_id: 28,
            wait_ms: -13,
            stance: true,
            flags: -17,
        });
        let steps = human.anim_steps;
        human.anim_step = 4;
        backend.run().unwrap();
        let human = &backend.human;
        assert_eq!(
            (
                human.car(),
                human.using_car(),
                human.action_state(),
                human.vehicle_action
            ),
            (None, None, HUMAN_ACTION_STAND, 0)
        );
        assert_eq!(
            (human.anim_id(), human.next_anim_id(), human.seat()),
            (0, ANIM_BREATH01A, -29)
        );
        assert_eq!(human.velocity().map(f32::to_bits), [0; 3]);
        assert_eq!(human.turn_speed().to_bits(), 0);
        assert!(!human.guarding());
        assert!(human.crouching() && human.aimed() && human.fire_held);
        macro_rules! existing_fields_equal {
            ($($field:ident => $value:expr),* $(,)?) => { $(assert_eq!(human.$field, $value, stringify!($field));)* };
        }
        existing_fields_equal!(sniper_mode=>false, agony_anim=>-1, fire_target=>None,
            no_anim_hit=>false, note_fire=>false, reloading=>false, reload_forced=>false,
            loading_shells=>false, idle_anims=>false, test_ground=>true, voice_priority=>-1,
            killed_by_shot=>false, drowned=>false, changing_weapon=>false, speed_limiter=>false,
            strike_anim_time=>-1, score_bonus=>false, weapon_displaced=>false, reload_effects=>false,
            shut_up=>false);
        assert_eq!(human.wingman, -1);
        assert_eq!(human.wingman_name, 0xface_0123);
        assert_eq!(
            (
                human.track_steps,
                human.track_steps_max,
                human.track_texture
            ),
            (0, i16::MAX, 0xff)
        );
        assert_eq!(human.agony_count, 98);
        assert_eq!(
            human.dir.map(f32::to_bits),
            [-0.0f32, 9.0, -2.0].map(f32::to_bits)
        );
        assert_eq!(human.carried_box(), HumanCarriedFrame::Absent);
        assert_eq!(human.carried_dead_body(), HumanCarriedFrame::Absent);
        assert_eq!(human.look_frame(), None);
        assert_eq!(human.anim_step, 0);
        assert_eq!(human.anim_steps[0].step_type, HUMAN_STEP_END);
        assert_eq!(human.anim_steps[0].anim_id, steps[0].anim_id);
        assert_eq!(human.anim_steps[0].wait_ms, steps[0].wait_ms);
        assert_eq!(human.anim_steps[0].stance, steps[0].stance);
        assert_eq!(human.anim_steps[0].flags, steps[0].flags);
        assert_eq!(&human.anim_steps[1..], &steps[1..]);
        let fields = human.game_init_fields();
        macro_rules! fields_equal {
            ($($field:ident => $value:expr),* $(,)?) => { $(assert_eq!(fields.$field, Some($value), stringify!($field));)* };
        }
        fields_equal!(anim_set=>0, last_velocity=>[0.0; 3], fall=>0.0, velocity_ramp=>0.0,
            idle_time=>-1, breath_time=>-1, striking=>false, dead_time=>500, last_fire_time=>0,
            track_test=>0, bleed_count=>0, human_flags=>0, move_time=>0, move_ratio=>1.0,
            step_time=>0, last_hit_time=>0, last_hit_render=>0, dead_body_time=>-1,
            lower_coll_off=>false, death_pos_set=>false, freeze_time=>-1, effect_step=>0,
            pump_time=>-1, scream_areas=>1, next_car_action=>0, car_action_time=>-1,
            temp_driver=>false, ab_created=>true, strike_time=>0, shots=>1.0, hits=>1.0,
            prev_shots=>1.0, prev_hits=>1.0, ratio_time=>5000, car_stance=>0,
            car_stance_time=>0, car_anim_index=>0, car_anim_length=>0.0, car_aim=>0.5,
            pose_moving=>false);
        assert_eq!(
            fields.move_pos.unwrap().map(f32::to_bits),
            backend.position.map(f32::to_bits)
        );
        assert_eq!(fields.traffic, HumanNullReference::Null);
        assert_eq!(fields.move_coll, HumanNullReference::Null);
        assert!(matches!(
            fields.orig_model,
            HumanGameInitValue::Established(None)
        ));
        assert!(matches!(
            fields.dead_body_human,
            HumanGameInitValue::Established(None)
        ));
        assert!(matches!(
            fields.stealing_car,
            HumanGameInitValue::Established(None)
        ));
        assert!(matches!(
            fields.hand_clip,
            HumanGameInitValue::Established(None)
        ));
        assert!(matches!(
            fields.killer,
            HumanGameInitValue::Established(None)
        ));
        assert!(matches!(
            fields.no_fuel_car,
            HumanGameInitValue::Established(None)
        ));
        assert!(matches!(
            fields.last_car,
            HumanGameInitValue::Established(None)
        ));
        assert!(matches!(
            fields.locked_car,
            HumanGameInitValue::Established(None)
        ));
        assert!(matches!(
            fields.locked_vehicle,
            HumanGameInitValue::Established(None)
        ));
    }

    #[test]
    fn animation_init_resets_only_evidenced_playback_and_clears_middle_word() {
        let mut backend = Backend::new();
        backend.run().unwrap();
        let machine = backend.human.animation_machine();
        assert_eq!(machine.untouched_constructor_word(), 0);
        let playback = machine.playback().unwrap();
        assert_eq!(playback, &HumanAnimationPlayback::initialized());
        assert_eq!(playback.blend.to_bits(), 0);
        assert_eq!(playback.blend_speed.to_bits(), 0);
        assert_eq!(playback.next_start.to_bits(), 0xbf80_0000);
        assert_eq!(playback.unknown_10e, u16::MAX);
        assert_eq!((playback.anim0_id, playback.anim1_id), (u32::MAX, u32::MAX));
    }

    #[test]
    fn missing_frame_faults_at_unguarded_find_after_base_effect() {
        let mut backend = Backend::new();
        backend.frame = None;
        assert_eq!(
            backend.run(),
            Err(HumanGameInitError::MissingFrame {
                stage: HumanGameInitStage::FindBaseFrame
            })
        );
        assert_eq!(*backend.events.borrow(), vec![Event::Base, Event::Frame]);
        assert_eq!(backend.human.base_frame(), Some(frame(802)));
        assert!(backend.human.alive());
        assert_eq!(backend.human.game_init_fields.idle_time, None);
    }

    #[test]
    fn missing_found_base_is_stored_and_does_not_skip_remaining_prefix() {
        let mut backend = Backend::new();
        backend.base = None;
        backend.run().unwrap();
        assert_eq!(backend.human.base_frame(), None);
        assert_eq!(backend.poses.len(), 3);
    }

    #[test]
    fn null_after_find_skips_retain_delete_but_runs_null_helpers() {
        let mut backend = Backend::new();
        backend.null_after_find = true;
        assert_eq!(
            backend.run(),
            Err(HumanGameInitError::MissingFrame {
                stage: HumanGameInitStage::SetBack1Pose
            })
        );
        let events = backend.events.borrow();
        assert!(!events
            .iter()
            .any(|event| matches!(event, Event::Retain(_) | Event::Delete(_))));
        assert!(events.contains(&Event::Debustize(None)));
        assert!(events.contains(&Event::NoShadow(None)));
        assert_eq!(backend.human.animation_machine().model(), None);
        assert!(matches!(
            backend.human.game_init_fields.pose_back1,
            HumanGameInitValue::Established(Some(_))
        ));
    }

    #[test]
    fn delete_receives_reloaded_nullable_frame_without_eager_null_fault() {
        let mut backend = Backend::new();
        backend.null_after_retain = true;
        backend.run().unwrap_err();
        assert!(backend.events.borrow().contains(&Event::Delete(None)));
        assert!(backend.events.borrow().contains(&Event::Debustize(None)));
    }

    #[test]
    fn nullable_pose_is_forwarded_for_all_three_slots() {
        for index in 0..3 {
            let mut backend = Backend::new();
            backend.null_pose = Some(index);
            backend.run().unwrap();
            let name = [b"back1".as_slice(), b"back2".as_slice(), b"neck".as_slice()][index];
            assert!(backend
                .events
                .borrow()
                .contains(&Event::Set(frame(1), name.to_vec(), false)));
        }
    }

    #[test]
    fn pose_is_stored_before_null_model_fault_and_finish_is_not_run() {
        for index in 0..3 {
            let mut backend = Backend::new();
            backend.null_in_create = Some(index);
            backend.human.look_frame = Some(frame(44));
            backend.human.anim_step = 4;
            let stage = [
                HumanGameInitStage::SetBack1Pose,
                HumanGameInitStage::SetBack2Pose,
                HumanGameInitStage::SetNeckPose,
            ][index];
            assert_eq!(
                backend.run(),
                Err(HumanGameInitError::MissingFrame { stage })
            );
            let fields = backend.human.game_init_fields();
            let stored = [&fields.pose_back1, &fields.pose_back2, &fields.pose_neck][index];
            match stored {
                HumanGameInitValue::Established(Some(stored)) => {
                    assert!(Rc::ptr_eq(stored, backend.poses[index].as_ref().unwrap()))
                }
                _ => panic!("allocated pose lost before native null dereference"),
            }
            assert_eq!(backend.human.look_frame(), Some(frame(44)));
            assert_eq!(backend.human.anim_step, 4);
            assert_eq!(backend.poses.len(), index + 1);
            assert!(!backend.events.borrow().iter().any(|event| matches!(event, Event::Set(_, name, _) if name == [b"back1".to_vec(), b"back2".to_vec(), b"neck".to_vec()][index].as_slice())));
        }
    }

    #[test]
    fn position_read_failure_preserves_exact_pre_read_reset_prefix() {
        let mut backend = Backend::new();
        backend.fail_read = Some(HumanGameInitStage::ReadPosition);
        backend.human.guarding = true;
        backend.human.track_steps = -51;
        backend.human.game_init_fields.idle_time = Some(66);
        backend.human.game_init_fields.move_pos = Some([17.0; 3]);
        assert_eq!(
            backend.run(),
            Err(HumanGameInitError::Backend {
                stage: HumanGameInitStage::ReadPosition,
                source: HumanGameInitStage::ReadPosition
            })
        );
        assert!(!backend.human.guarding());
        assert_eq!(backend.human.velocity().map(f32::to_bits), [0; 3]);
        assert_eq!(
            backend
                .human
                .game_init_fields
                .last_velocity
                .unwrap()
                .map(f32::to_bits),
            [0; 3]
        );
        assert_eq!(
            backend.human.current_properties(),
            backend.human.properties()
        );
        assert_eq!(backend.human.game_init_fields.idle_time, Some(66));
        assert_eq!(backend.human.game_init_fields.move_pos, Some([17.0; 3]));
        assert_eq!(backend.human.track_steps, -51);
        assert!(backend.poses.is_empty());
    }

    #[test]
    fn animation_frame_read_failure_keeps_prior_stores_without_inventing_playback() {
        let mut backend = Backend::new();
        backend.fail_read = Some(HumanGameInitStage::AnimationInit);
        backend.human.next_anim_id = 37;
        assert_eq!(
            backend.run(),
            Err(HumanGameInitError::Backend {
                stage: HumanGameInitStage::AnimationInit,
                source: HumanGameInitStage::AnimationInit
            })
        );
        assert_eq!(
            backend.human.animation_machine().owner(),
            Some(backend.actor)
        );
        assert_eq!(
            backend
                .human
                .animation_machine()
                .untouched_constructor_word(),
            0
        );
        assert!(backend.human.animation_machine().playback().is_none());
        assert_eq!(backend.human.next_anim_id(), 37);
        assert_eq!(backend.human.current_properties().record(), &[0xda; 64]);
    }

    #[test]
    fn every_dependency_failure_retains_started_prefix_and_refuses_replay() {
        use HumanGameInitStage as Stage;
        for stage in [
            Stage::BaseActor,
            Stage::FindBaseFrame,
            Stage::RetainFrame,
            Stage::DeleteSceneFrame,
            Stage::DebustizeFrame,
            Stage::NoShadowFrame,
            Stage::CreateBack1Pose,
            Stage::SetBack1Pose,
            Stage::CreateBack2Pose,
            Stage::SetBack2Pose,
            Stage::CreateNeckPose,
            Stage::SetNeckPose,
        ] {
            let mut backend = Backend::new();
            backend.fail = Some(stage);
            backend.human.look_frame = Some(frame(78));
            assert_eq!(
                backend.run(),
                Err(HumanGameInitError::Backend {
                    stage,
                    source: stage
                })
            );
            assert_eq!(
                backend.human.game_init_status(),
                HumanGameInitStatus::Started(stage)
            );
            let events = backend.events.borrow().clone();
            assert_eq!(
                backend.run(),
                Err(HumanGameInitError::AlreadyAttempted {
                    status: HumanGameInitStatus::Started(stage)
                })
            );
            assert_eq!(*backend.events.borrow(), events);
            assert_eq!(backend.human.look_frame(), Some(frame(78)));
        }
    }

    #[test]
    fn panic_after_native_effect_also_fences_automatic_replay() {
        let mut backend = Backend::new();
        backend.panic_base = true;
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| backend.run())).is_err());
        assert!(backend.human.alive());
        assert_eq!(
            backend.run(),
            Err(HumanGameInitError::AlreadyAttempted {
                status: HumanGameInitStatus::Started(HumanGameInitStage::BaseActor)
            })
        );
        assert_eq!(*backend.events.borrow(), vec![Event::Base]);
    }

    #[test]
    fn boundary_is_not_complete_or_replayable_and_has_no_player_suffix() {
        let mut backend = Backend::new();
        backend.run().unwrap();
        let status = HumanGameInitStatus::AtBoundary(HumanGameInitBoundary::CreateDynColls);
        assert_eq!(backend.human.game_init_status(), status);
        assert!(!backend.human.idle_anims());
        let events = backend.events.borrow().clone();
        assert_eq!(
            backend.run(),
            Err(HumanGameInitError::AlreadyAttempted { status })
        );
        assert_eq!(*backend.events.borrow(), events);
    }

    #[test]
    fn carried_frame_ids_and_legacy_presence_share_the_same_slot() {
        let mut backend = Backend::new();
        backend.human.set_carried_box_frame(Some(frame(83)));
        backend.human.set_carried_dead_body_frame(Some(frame(84)));
        assert_eq!(
            backend.human.carried_box(),
            HumanCarriedFrame::Frame(frame(83))
        );
        assert_eq!(
            backend.human.carried_dead_body(),
            HumanCarriedFrame::Frame(frame(84))
        );
        assert!(backend.human.carrying_box() && backend.human.carrying_dead_body());
        backend.human.set_carrying_box(true);
        assert_eq!(
            backend.human.carried_box(),
            HumanCarriedFrame::UnresolvedPresence
        );
        backend.human.set_carrying_dead_body(false);
        assert_eq!(backend.human.carried_dead_body(), HumanCarriedFrame::Absent);
        backend.run().unwrap();
        assert!(!backend.human.carrying_box() && !backend.human.carrying_dead_body());
    }

    fn collision_frames(human: &HumanState) -> &HumanCollisionFrameSlots {
        match &human.dynamic_collisions().coll_frames {
            HumanGameInitValue::Established(Some(frames)) => frames,
            _ => panic!("frame allocation is not established"),
        }
    }

    fn collisions(human: &HumanState) -> &HumanDynamicCollisionSlots {
        match &human.dynamic_collisions().colls {
            HumanGameInitValue::Established(Some(slots)) => slots,
            _ => panic!("collision allocation is not established"),
        }
    }

    #[test]
    fn lower_collision_setup_owns_resources_and_preserves_exact_order_and_bits() {
        use HumanGameInitValue::{Established, Unestablished};

        let mut backend = Backend::at_collision_boundary();
        assert!(matches!(
            backend.human.dynamic_collisions().colls,
            Unestablished
        ));
        assert_eq!(
            backend.run_collision(),
            Ok(HumanGameInitBoundary::InsertLowerDynamicCollision)
        );
        assert_eq!(
            *backend.events.borrow(),
            vec![
                Event::AllocateCollisions(2),
                Event::AllocateCollisionFrames(2),
                Event::Frame,
                Event::AllocateLowerCollision,
                Event::WorldPosition(frame(1)),
                Event::Frame,
            ]
        );
        assert_eq!(backend.human.dynamic_collisions().num_colls, Established(2));
        assert!(matches!(
            collisions(&backend.human).slots[1],
            Established(None)
        ));
        assert_eq!(
            collision_frames(&backend.human).slots.as_ref(),
            [Established(Some(frame(1))), Unestablished]
        );
        let lower = backend.lower_collision();
        assert!(Rc::ptr_eq(
            &lower,
            backend.collision.allocated_collision.as_ref().unwrap()
        ));
        let collision = lower.borrow();
        assert_eq!(collision.kind, 0x42);
        assert_eq!(collision.flags, 0x101);
        assert_eq!(collision.frame, Some(frame(1)));
        assert_eq!(collision.owner, Some(backend.actor));
        assert_eq!(
            collision.sphere_position.map(f32::to_bits),
            backend.position.map(f32::to_bits)
        );
        assert_eq!(collision.sphere_radius.to_bits(), 0x3e99_999a);
        assert_eq!(collision.cell, HumanNullReference::Null);
        assert_eq!(collision.matrix, HumanNullReference::Null);
        assert_eq!(collision.inverse_matrix, HumanNullReference::Null);
        assert_eq!(collision.unknown_20, 0);
        assert_eq!(
            collision.bounding_box.map(|point| point.map(f32::to_bits)),
            [[0; 3]; 2]
        );
        assert!(!backend.human.colls_active());
        assert_eq!(backend.poses.len(), 3);
        assert_eq!(
            backend.human.game_init_status(),
            HumanGameInitStatus::AtBoundary(HumanGameInitBoundary::InsertLowerDynamicCollision)
        );
    }

    #[test]
    fn lower_collision_resume_requires_the_exact_boundary_and_live_owner() {
        let mut backend = Backend::new();
        assert_eq!(
            backend.run_collision(),
            Err(HumanGameInitError::UnexpectedBoundary {
                expected: HumanGameInitBoundary::CreateDynColls,
                status: HumanGameInitStatus::NotStarted,
            })
        );
        assert!(backend.events.borrow().is_empty());
        let mut backend = Backend::at_collision_boundary();
        assert_eq!(
            game_init_through_lower_collision_setup(
                &mut backend,
                ActorReferenceId::from_existing(18)
            ),
            Err(HumanGameInitError::Backend {
                stage: HumanGameInitStage::EnterDynColls,
                source: HumanGameInitStage::Enter,
            })
        );
        assert!(backend.events.borrow().is_empty());
        assert_eq!(
            backend.human.game_init_status(),
            HumanGameInitStatus::AtBoundary(HumanGameInitBoundary::CreateDynColls)
        );
        backend.run_collision().unwrap();
        let events = backend.events.borrow().clone();
        let status = backend.human.game_init_status();
        assert_eq!(
            backend.run_collision(),
            Err(HumanGameInitError::UnexpectedBoundary {
                expected: HumanGameInitBoundary::CreateDynColls,
                status,
            })
        );
        assert_eq!(
            backend.run(),
            Err(HumanGameInitError::AlreadyAttempted { status })
        );
        assert_eq!(*backend.events.borrow(), events);
    }

    #[test]
    fn lower_collision_null_allocations_store_null_before_the_reached_error() {
        use HumanDynCollsStorageError as Storage;
        use HumanGameInitStage as Stage;
        use HumanGameInitValue::Established;

        for (allocation, stage, source, events) in [
            (
                0,
                Stage::ClearCollisionSlots,
                Storage::NullCollisionSlots,
                vec![Event::AllocateCollisions(2)],
            ),
            (
                1,
                Stage::SetLowerCollisionFrame,
                Storage::NullFrameSlots,
                vec![
                    Event::AllocateCollisions(2),
                    Event::AllocateCollisionFrames(2),
                    Event::Frame,
                ],
            ),
            (
                2,
                Stage::ClearLowerDynamicCollision,
                Storage::NullCollision { index: 0 },
                vec![
                    Event::AllocateCollisions(2),
                    Event::AllocateCollisionFrames(2),
                    Event::Frame,
                    Event::AllocateLowerCollision,
                ],
            ),
        ] {
            let mut backend = Backend::at_collision_boundary();
            backend.collision.null_slots = allocation == 0;
            backend.collision.null_frames = allocation == 1;
            backend.collision.null_collision = allocation == 2;
            assert_eq!(
                backend.run_collision(),
                Err(HumanGameInitError::CollisionStorage { stage, source })
            );
            assert_eq!(*backend.events.borrow(), events);
            match allocation {
                0 => assert!(matches!(
                    backend.human.dynamic_collisions().colls,
                    Established(None)
                )),
                1 => assert!(matches!(
                    backend.human.dynamic_collisions().coll_frames,
                    Established(None)
                )),
                _ => assert!(matches!(
                    collisions(&backend.human).slots[0],
                    Established(None)
                )),
            }
            assert_eq!(
                backend.human.game_init_status(),
                HumanGameInitStatus::Started(stage)
            );
            assert!(!backend.human.colls_active());
            assert!(matches!(
                backend.run_collision(),
                Err(HumanGameInitError::UnexpectedBoundary { .. })
            ));
            assert_eq!(*backend.events.borrow(), events);
        }
    }

    #[test]
    fn lower_collision_backend_failures_keep_all_reached_storage_and_forbid_replay() {
        use HumanGameInitStage as Stage;
        use HumanGameInitValue::{Established, Unestablished};

        for (stage, calls) in [
            (Stage::AllocateCollisionSlots, 1),
            (Stage::AllocateCollisionFrameSlots, 2),
            (Stage::AllocateLowerDynamicCollision, 4),
            (Stage::ReadLowerWorldPosition, 5),
        ] {
            let mut backend = Backend::at_collision_boundary();
            backend.fail = Some(stage);
            assert_eq!(
                backend.run_collision(),
                Err(HumanGameInitError::Backend {
                    stage,
                    source: stage
                })
            );
            assert_eq!(backend.events.borrow().len(), calls);
            assert_eq!(backend.human.dynamic_collisions().num_colls, Established(2));
            if stage == Stage::AllocateCollisionSlots {
                assert!(matches!(
                    backend.human.dynamic_collisions().colls,
                    Unestablished
                ));
            } else if stage != Stage::ReadLowerWorldPosition {
                assert!(matches!(
                    collisions(&backend.human).slots[0],
                    Established(None)
                ));
            } else {
                let collision = backend.lower_collision();
                let collision = collision.borrow();
                assert_eq!((collision.kind, collision.flags), (0x42, 0x101));
                assert_eq!(collision.sphere_radius.to_bits(), 0);
                assert_eq!(collision.frame, None);
                assert_eq!(collision.owner, None);
            }
            assert_eq!(
                backend.human.game_init_status(),
                HumanGameInitStatus::Started(stage)
            );
            let events = backend.events.borrow().clone();
            assert!(matches!(
                backend.run_collision(),
                Err(HumanGameInitError::UnexpectedBoundary { .. })
            ));
            assert_eq!(*backend.events.borrow(), events);
        }
    }

    #[test]
    fn lower_collision_missing_cached_frame_is_checked_only_after_lower_setup() {
        let mut backend = Backend::at_collision_boundary();
        backend.frame = None;
        assert_eq!(
            backend.run_collision(),
            Err(HumanGameInitError::MissingFrame {
                stage: HumanGameInitStage::ReadLowerWorldPosition
            })
        );
        assert_eq!(
            backend.events.borrow().last(),
            Some(&Event::AllocateLowerCollision)
        );
        assert_eq!(
            collision_frames(&backend.human).slots[0],
            HumanGameInitValue::Established(None)
        );
        let lower = backend.lower_collision();
        let collision = lower.borrow();
        assert_eq!((collision.kind, collision.flags), (0x42, 0x101));
        assert_eq!(collision.sphere_radius.to_bits(), 0);
        assert_eq!(collision.owner, None);
    }

    #[test]
    fn lower_collision_rereads_count_after_allocation_and_preserves_upper_storage() {
        use HumanGameInitValue::{Established, Unestablished};

        let mut backend = Backend::at_collision_boundary();
        backend.collision.on_slots =
            Some(|backend| backend.human.dynamic_collisions_mut().num_colls = Established(1));
        backend.run_collision().unwrap();
        assert!(backend
            .events
            .borrow()
            .contains(&Event::AllocateCollisionFrames(1)));
        assert!(matches!(collisions(&backend.human).slots[1], Unestablished));
        assert_eq!(collision_frames(&backend.human).slots.len(), 1);
        assert_eq!(backend.human.dynamic_collisions().num_colls, Established(1));
    }

    #[test]
    fn lower_collision_native_pointer_byte_count_wraps_in_u32() {
        let mut backend = Backend::at_collision_boundary();
        backend.collision.on_slots = Some(|backend| {
            backend.human.dynamic_collisions_mut().num_colls =
                HumanGameInitValue::Established(0x4000_0002)
        });
        backend.run_collision().unwrap();
        assert!(backend
            .events
            .borrow()
            .contains(&Event::AllocateCollisionFrames(2)));
        assert!(matches!(
            collisions(&backend.human).slots[1],
            HumanGameInitValue::Established(None)
        ));
        assert_eq!(
            backend.human.dynamic_collisions().num_colls,
            HumanGameInitValue::Established(0x4000_0002)
        );
    }

    #[test]
    fn lower_collision_clear_overrun_keeps_the_valid_cleared_prefix() {
        let mut backend = Backend::at_collision_boundary();
        backend.collision.on_slots = Some(|backend| {
            backend.human.dynamic_collisions_mut().num_colls = HumanGameInitValue::Established(3)
        });
        assert_eq!(
            backend.run_collision(),
            Err(HumanGameInitError::CollisionStorage {
                stage: HumanGameInitStage::ClearCollisionSlots,
                source: HumanDynCollsStorageError::CollisionSlotOutOfBounds { index: 2, len: 2 },
            })
        );
        assert!(collisions(&backend.human)
            .slots
            .iter()
            .all(|slot| matches!(slot, HumanGameInitValue::Established(None))));
        assert_eq!(*backend.events.borrow(), vec![Event::AllocateCollisions(2)]);
    }

    #[test]
    fn lower_collision_zero_byte_clear_does_not_dereference_null() {
        let mut backend = Backend::at_collision_boundary();
        backend.collision.null_slots = true;
        backend.collision.on_slots = Some(|backend| {
            backend.human.dynamic_collisions_mut().num_colls =
                HumanGameInitValue::Established(0x4000_0000)
        });
        assert_eq!(
            backend.run_collision(),
            Err(HumanGameInitError::CollisionStorage {
                stage: HumanGameInitStage::SetLowerCollisionFrame,
                source: HumanDynCollsStorageError::FrameSlotOutOfBounds { index: 0, len: 0 },
            })
        );
        assert_eq!(
            *backend.events.borrow(),
            vec![
                Event::AllocateCollisions(2),
                Event::AllocateCollisionFrames(0),
                Event::Frame
            ]
        );
        assert!(matches!(
            backend.human.dynamic_collisions().colls,
            HumanGameInitValue::Established(None)
        ));
        assert!(collision_frames(&backend.human).slots.is_empty());
    }

    #[test]
    fn lower_collision_unestablished_count_stops_before_pointer_store() {
        let mut backend = Backend::at_collision_boundary();
        backend.collision.on_slots = Some(|backend| {
            backend.human.dynamic_collisions_mut().num_colls = HumanGameInitValue::Unestablished
        });
        assert_eq!(
            backend.run_collision(),
            Err(HumanGameInitError::CollisionStorage {
                stage: HumanGameInitStage::ClearCollisionSlots,
                source: HumanDynCollsStorageError::UnestablishedCount,
            })
        );
        assert!(matches!(
            backend.human.dynamic_collisions().colls,
            HumanGameInitValue::Unestablished
        ));
        assert_eq!(*backend.events.borrow(), vec![Event::AllocateCollisions(2)]);
    }

    #[test]
    fn lower_collision_caches_source_frame_but_rereads_actor_frame_after_callbacks() {
        let mut backend = Backend::at_collision_boundary();
        backend.collision.on_frames = Some(|backend| backend.frame = Some(frame(2)));
        backend.collision.on_collision = Some(|backend| backend.frame = Some(frame(3)));
        backend.collision.on_world = Some(|backend| backend.frame = Some(frame(4)));
        backend.run_collision().unwrap();
        assert!(backend
            .events
            .borrow()
            .contains(&Event::WorldPosition(frame(2))));
        assert_eq!(
            collision_frames(&backend.human).slots[0],
            HumanGameInitValue::Established(Some(frame(2)))
        );
        assert_eq!(backend.lower_collision().borrow().frame, Some(frame(4)));
    }

    #[test]
    fn lower_collision_frame_read_failure_precedes_frame_array_commit() {
        use HumanGameInitValue::Established;

        let mut backend = Backend::at_collision_boundary();
        backend.collision.on_frames = Some(|backend| {
            let mut frames = HumanCollisionFrameSlots::try_allocate(1).unwrap();
            frames.slots[0] = Established(Some(frame(91)));
            backend.human.dynamic_collisions_mut().coll_frames = Established(Some(frames));
        });
        backend.fail_read = Some(HumanGameInitStage::SetLowerCollisionFrame);
        assert_eq!(
            backend.run_collision(),
            Err(HumanGameInitError::Backend {
                stage: HumanGameInitStage::SetLowerCollisionFrame,
                source: HumanGameInitStage::SetLowerCollisionFrame,
            })
        );
        assert_eq!(
            collision_frames(&backend.human).slots.as_ref(),
            [Established(Some(frame(91)))]
        );
        assert_eq!(
            *backend.events.borrow(),
            vec![
                Event::AllocateCollisions(2),
                Event::AllocateCollisionFrames(2),
                Event::Frame
            ]
        );
    }

    #[test]
    fn lower_collision_allocation_callback_redirects_the_actual_destination_slots() {
        use HumanGameInitValue::{Established, Unestablished};

        let mut backend = Backend::at_collision_boundary();
        backend.collision.on_collision = Some(|backend| {
            let slots = HumanDynamicCollisionSlots::try_allocate(3).unwrap();
            backend.human.dynamic_collisions_mut().colls = Established(Some(slots));
        });
        backend.run_collision().unwrap();
        assert_eq!(collisions(&backend.human).slots.len(), 3);
        assert!(matches!(collisions(&backend.human).slots[1], Unestablished));
        assert!(matches!(collisions(&backend.human).slots[2], Unestablished));
        assert!(Rc::ptr_eq(
            &backend.lower_collision(),
            backend.collision.allocated_collision.as_ref().unwrap()
        ));
    }

    #[test]
    fn lower_collision_world_callback_redirects_resources_without_resetting_replacement() {
        use HumanGameInitValue::Established;

        let mut backend = Backend::at_collision_boundary();
        backend.collision.on_world = Some(|backend| {
            let old = backend.lower_collision();
            assert!(old.try_borrow_mut().is_ok());
            assert_eq!(old.borrow().kind, 0x42);
            assert_eq!(old.borrow().flags, 0x101);
            let mut replacement = HumanDynamicCollision::zeroed();
            replacement.kind = 0x88;
            replacement.flags = 0x1234;
            replacement.unknown_20 = 0xfedc_ba98;
            replacement.bounding_box = [[-0.0; 3], [f32::from_bits(0x7fa1_2345); 3]];
            let mut slots = HumanDynamicCollisionSlots::try_allocate(1).unwrap();
            slots.slots[0] = Established(Some(Rc::new(RefCell::new(replacement))));
            backend.human.dynamic_collisions_mut().colls = Established(Some(slots));
            let mut frames = HumanCollisionFrameSlots::try_allocate(1).unwrap();
            frames.slots[0] = Established(Some(frame(99)));
            backend.human.dynamic_collisions_mut().coll_frames = Established(Some(frames));
            backend.frame = Some(frame(77));
        });
        backend.run_collision().unwrap();
        let lower = backend.lower_collision();
        let current = lower.borrow();
        assert!(!Rc::ptr_eq(
            &lower,
            backend.collision.allocated_collision.as_ref().unwrap()
        ));
        assert_eq!(
            (current.kind, current.flags, current.unknown_20),
            (0x88, 0x1234, 0xfedc_ba98)
        );
        assert_eq!(
            current.bounding_box.map(|point| point.map(f32::to_bits)),
            [[0x8000_0000; 3], [0x7fa1_2345; 3]]
        );
        assert_eq!(
            current.sphere_position.map(f32::to_bits),
            backend.position.map(f32::to_bits)
        );
        assert_eq!(current.sphere_radius.to_bits(), 0x3e99_999a);
        assert_eq!(current.frame, Some(frame(77)));
        assert_eq!(current.owner, Some(backend.actor));
        let original = backend
            .collision
            .allocated_collision
            .as_ref()
            .unwrap()
            .borrow();
        assert_eq!((original.kind, original.flags), (0x42, 0x101));
        assert_eq!(original.sphere_position.map(f32::to_bits), [0; 3]);
        assert_eq!(original.sphere_radius.to_bits(), 0);
        assert_eq!(original.frame, None);
        assert_eq!(original.owner, None);
        assert_eq!(
            *backend.events.borrow(),
            vec![
                Event::AllocateCollisions(2),
                Event::AllocateCollisionFrames(2),
                Event::Frame,
                Event::AllocateLowerCollision,
                Event::WorldPosition(frame(1)),
                Event::Frame
            ]
        );
    }

    #[test]
    fn lower_collision_world_callback_invalidates_only_the_reached_destination() {
        use HumanDynCollsStorageError as Storage;
        use HumanGameInitValue::{Established, Unestablished};

        type StorageCase = (fn(&mut Backend), Storage);
        let cases: [StorageCase; 5] = [
            (
                |backend| backend.human.dynamic_collisions_mut().colls = Unestablished,
                Storage::UnestablishedCollisionSlots,
            ),
            (
                |backend| backend.human.dynamic_collisions_mut().colls = Established(None),
                Storage::NullCollisionSlots,
            ),
            (
                |backend| {
                    backend.human.dynamic_collisions_mut().colls =
                        Established(Some(HumanDynamicCollisionSlots::try_allocate(0).unwrap()))
                },
                Storage::CollisionSlotOutOfBounds { index: 0, len: 0 },
            ),
            (
                |backend| {
                    backend.human.dynamic_collisions_mut().colls =
                        Established(Some(HumanDynamicCollisionSlots::try_allocate(1).unwrap()))
                },
                Storage::UnestablishedCollision { index: 0 },
            ),
            (
                |backend| collisions_mut_first(backend, Established(None)),
                Storage::NullCollision { index: 0 },
            ),
        ];
        for (callback, source) in cases {
            let mut backend = Backend::at_collision_boundary();
            backend.collision.on_world = Some(callback);
            assert_eq!(
                backend.run_collision(),
                Err(HumanGameInitError::CollisionStorage {
                    stage: HumanGameInitStage::SetLowerCollisionSphere,
                    source,
                })
            );
            assert_eq!(
                backend.events.borrow().last(),
                Some(&Event::WorldPosition(frame(1)))
            );
            assert_eq!(backend.events.borrow().len(), 5);
            let original = backend
                .collision
                .allocated_collision
                .as_ref()
                .unwrap()
                .borrow();
            assert_eq!(original.sphere_position.map(f32::to_bits), [0; 3]);
            assert_eq!(original.sphere_radius.to_bits(), 0);
            assert_eq!(original.owner, None);
        }
    }

    fn collisions_mut_first(
        backend: &mut Backend,
        value: HumanGameInitValue<Option<HumanDynamicCollisionHandle>>,
    ) {
        collision_slots_mut(&mut backend.human).unwrap().slots[0] = value;
    }

    #[test]
    fn lower_collision_frame_storage_is_read_lazily_after_lower_kind_and_flags() {
        use HumanDynCollsStorageError as Storage;
        use HumanGameInitValue::{Established, Unestablished};

        type StorageCase = (fn(&mut Backend), Storage);
        let cases: [StorageCase; 4] = [
            (
                |backend| backend.human.dynamic_collisions_mut().coll_frames = Unestablished,
                Storage::UnestablishedFrameSlots,
            ),
            (
                |backend| backend.human.dynamic_collisions_mut().coll_frames = Established(None),
                Storage::NullFrameSlots,
            ),
            (
                |backend| {
                    backend.human.dynamic_collisions_mut().coll_frames =
                        Established(Some(HumanCollisionFrameSlots::try_allocate(0).unwrap()))
                },
                Storage::FrameSlotOutOfBounds { index: 0, len: 0 },
            ),
            (
                |backend| {
                    backend.human.dynamic_collisions_mut().coll_frames =
                        Established(Some(HumanCollisionFrameSlots::try_allocate(1).unwrap()))
                },
                Storage::UnestablishedFrame { index: 0 },
            ),
        ];
        for (callback, source) in cases {
            let mut backend = Backend::at_collision_boundary();
            backend.collision.on_collision = Some(callback);
            assert_eq!(
                backend.run_collision(),
                Err(HumanGameInitError::CollisionStorage {
                    stage: HumanGameInitStage::ReadLowerWorldPosition,
                    source,
                })
            );
            assert_eq!(backend.events.borrow().len(), 4);
            let lower = backend.lower_collision();
            let lower = lower.borrow();
            assert_eq!((lower.kind, lower.flags), (0x42, 0x101));
            assert_eq!(lower.sphere_radius.to_bits(), 0);
            assert_eq!(lower.owner, None);
        }
    }

    #[test]
    fn lower_collision_null_latest_actor_frame_is_a_store_not_a_source_failure() {
        let mut backend = Backend::at_collision_boundary();
        backend.collision.on_world = Some(|backend| backend.frame = None);
        backend.run_collision().unwrap();
        let lower = backend.lower_collision();
        let lower = lower.borrow();
        assert_eq!(lower.frame, None);
        assert_eq!(lower.owner, Some(backend.actor));
        assert_eq!(lower.sphere_radius.to_bits(), 0x3e99_999a);
    }

    #[test]
    fn lower_collision_late_frame_failure_keeps_the_sphere_but_not_owner_store() {
        let mut backend = Backend::at_collision_boundary();
        backend.fail_read = Some(HumanGameInitStage::SetLowerDynamicCollisionFrame);
        assert_eq!(
            backend.run_collision(),
            Err(HumanGameInitError::Backend {
                stage: HumanGameInitStage::SetLowerDynamicCollisionFrame,
                source: HumanGameInitStage::SetLowerDynamicCollisionFrame,
            })
        );
        let lower = backend.lower_collision();
        let lower = lower.borrow();
        assert_eq!(
            lower.sphere_position.map(f32::to_bits),
            backend.position.map(f32::to_bits)
        );
        assert_eq!(lower.sphere_radius.to_bits(), 0x3e99_999a);
        assert_eq!(lower.frame, None);
        assert_eq!(lower.owner, None);
        assert_eq!(backend.events.borrow().last(), Some(&Event::Frame));
    }

    #[test]
    fn lower_collision_borrow_conflict_is_an_explicit_interrupted_prefix() {
        let mut backend = Backend::at_collision_boundary();
        let resource = Rc::new(RefCell::new(HumanDynamicCollision::zeroed()));
        backend.collision.allocated_collision = Some(resource.clone());
        let borrow = resource.borrow();
        assert_eq!(
            backend.run_collision(),
            Err(HumanGameInitError::CollisionStorage {
                stage: HumanGameInitStage::ClearLowerDynamicCollision,
                source: HumanDynCollsStorageError::CollisionBorrowConflict { index: 0 },
            })
        );
        assert_eq!(borrow.kind, 0);
        assert!(Rc::ptr_eq(&resource, &backend.lower_collision()));
        assert_eq!(backend.events.borrow().len(), 4);
        assert!(matches!(
            backend.run_collision(),
            Err(HumanGameInitError::UnexpectedBoundary { .. })
        ));
    }

    #[test]
    fn lower_collision_callback_panic_does_not_replay_uncertain_effects() {
        let mut backend = Backend::at_collision_boundary();
        backend.collision.on_world = Some(|backend| {
            backend.frame = Some(frame(89));
            panic!("synthetic interrupted world-cache update");
        });
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| backend.run_collision()))
                .is_err()
        );
        assert_eq!(
            backend.human.game_init_status(),
            HumanGameInitStatus::Started(HumanGameInitStage::ReadLowerWorldPosition)
        );
        assert_eq!(backend.frame, Some(frame(89)));
        let events = backend.events.borrow().clone();
        assert!(matches!(
            backend.run_collision(),
            Err(HumanGameInitError::UnexpectedBoundary { .. })
        ));
        assert_eq!(*backend.events.borrow(), events);
        assert_eq!(
            backend.lower_collision().borrow().sphere_radius.to_bits(),
            0
        );
    }

    #[test]
    fn lower_collision_does_not_write_active_latch_or_round_world_position_bits() {
        for active in [false, true] {
            let mut backend = Backend::at_collision_boundary();
            backend.human.colls_active = active;
            backend.position = [
                f32::from_bits(0x7f80_0001),
                -0.0,
                f32::from_bits(0x3f80_0001),
            ];
            backend.run_collision().unwrap();
            assert_eq!(backend.human.colls_active(), active);
            assert_eq!(
                backend
                    .lower_collision()
                    .borrow()
                    .sphere_position
                    .map(f32::to_bits),
                [0x7f80_0001, 0x8000_0000, 0x3f80_0001]
            );
        }
    }

    #[test]
    fn wrong_live_owner_is_rejected_before_effects_or_attempt_marker() {
        let mut backend = Backend::new();
        assert_eq!(
            game_init_through_pose_setup(&mut backend, ActorReferenceId::from_existing(18)),
            Err(HumanGameInitError::Backend {
                stage: HumanGameInitStage::Enter,
                source: HumanGameInitStage::Enter
            })
        );
        assert!(backend.events.borrow().is_empty());
        assert_eq!(
            backend.human.game_init_status(),
            HumanGameInitStatus::NotStarted
        );
    }
}
