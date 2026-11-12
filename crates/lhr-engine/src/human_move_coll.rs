//! Guarded prefix of C_human::TestMoveColl (GAME 0x57cb60, RVA 0x17cb60).
//! Game.exe SHA-256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27,
//! base 0x400000 (RVA = VA - 0x400000). Reviewed upstream source: mafia-re
//! 113b5b6 game/actors/C_human.cpp:5274 (~34 lines) with the tgcPanak2 shape
//! at game/actors/C_human.h:440 and the flag words at
//! game/collision/g_colls.h:31-37. Only the collision-descent-independent
//! leaves are owned here: the three capsule radii, the sphere lifts, the
//! gravity-biased sweep direction, the TestSphereB flag words and the
//! hit/miss result selection, assembled in the original's order by
//! test_move_coll_prefix. Vectors are plain [f32; 3] triples; the arithmetic
//! is component-wise exactly as the original's S_vector operations.
//! This file additionally owns the TestMoveCollInOwner probe leaves (GAME
//! 0x57d0e0, RVA 0x17d0e0, C_human.cpp:5329-5358: local-frame lifts, owner
//! radii, COLF_UNKNOWN_2-only flags, the car/railway seat dispatch gate and
//! the shared hit/miss result selection over the vehicle's dyn_colls slice)
//! and the collision-free Movement/MovementIntern control core (C_human.cpp:
//! 4752-4795 dispatch incl. 4757 fall seed, 4763-4772 void fall and 50.0 lethal
//! fall, 4774-4788 MovementIntern call plus move_ratio bookkeeping; 5037-5266
//! MovementIntern: 5041-5044 velocity/last_velocity displacement, 5064-5079
//! ground TestLineV probes with the 0.3 move_ratio step_actor arm and the 1.0
//! slope reject, 5081-5127 water/death/material/test_ground/fall-floor update,
//! 5128-5133 slope projection, 5136-5137 gravity, 5139-5148 small-step boost
//! and TestMoveColl/TestMoveCollInOwner dispatch, 5149-5256 water landing,
//! FALL landing, STAND pinning, fall reset and JUMP->STAND). World queries
//! (TestLineV/TestMoveColl sweeps, frame reads) arrive as caller-supplied
//! probe values; presentation, animation, sound, particle, police and Death/Hit
//! effects stay DEFERRED behind explicit outcome flags.
//! DEFERRED with explicit markers, no new owner here: the TestPanak2 capsule
//! sweep itself (no capsule owner exists), the frame anchoring
//! (GetWorldPos/GetWorldDir; no frame owner on this path), the
//! g_collision::DynSetNextCollOwnerMult call itself (no collision manager
//! owner; the colls + ghost_colls merge leaf is owned here and staged in
//! human_actor), the
//! act_mission/game player comparison behind is_player (no mission owner),
//! the move_coll writeback (C_human 0x414 word; no owner yet), the step_actor
//! handoff around the move_ratio>=0.3 ground probe (no collision manager
//! owner), camera/particle/sound/indicator/police effects, Hit, PersonAnim
//! and Death calls, the move_coll owner arms (bottle/grenade/car/bridge/
//! entity Collision/Event calls), and the Intern_UseCar full body (the seat
//! slice stays landed in human_actor).
#![forbid(unsafe_code)]

use crate::human_actor::{
    HUMAN_ACTION_FALL, HUMAN_ACTION_JUMP, HUMAN_ACTION_STAND, HUMAN_ACTION_WALK,
};

/// Capsule radii for legs, body and head (C_human.cpp:5286-5288).
pub const TEST_MOVE_COLL_RADII: [f32; 3] = [0.4, 0.25, 0.35];
/// Legs sphere lift above the frame position (C_human.cpp:5278: +0.405).
pub const TEST_MOVE_COLL_LEGS_LIFT: f32 = 0.405;
/// Result drop below the stopped legs sphere on a hit (C_human.cpp:5303:
/// pos_out - 0.4; the same value as the legs radius).
pub const TEST_MOVE_COLL_RESULT_DROP: f32 = 0.4;
/// Sweep-direction gravity bias (C_human.cpp:5291: dir.y -= 0.015).
pub const TEST_MOVE_COLL_DIR_Y_BIAS: f32 = -0.015;
/// Body sphere lift above the legs sphere, standing
/// (C_human.cpp:5280: +0.55).
pub const TEST_MOVE_COLL_BODY_LIFT: f32 = 0.55;
/// Head sphere lift above the legs sphere, standing (C_human.cpp:5281: +1.0).
pub const TEST_MOVE_COLL_HEAD_LIFT: f32 = 1.0;
/// Body sphere lift above the legs sphere, crouching
/// (C_human.cpp:5280: +0.3).
pub const TEST_MOVE_COLL_BODY_LIFT_CROUCH: f32 = 0.3;
/// Head sphere lift above the legs sphere, crouching
/// (C_human.cpp:5281: +0.6).
pub const TEST_MOVE_COLL_HEAD_LIFT_CROUCH: f32 = 0.6;

/// TestSphereB flags for the capsule (C_human.cpp:5292):
/// `(alive ? 0 : COLF_ALL) | COLF_UNKNOWN_80000 | COLF_UNKNOWN_2` with
/// COLF_ALL 0xffffffff, COLF_UNKNOWN_80000 0x80000 and COLF_UNKNOWN_2 2 per
/// mafia-re 113b5b6 game/collision/g_colls.h:31-34.
pub fn test_move_coll_flags(alive: bool) -> u32 {
    const ALL: u32 = 0xffff_ffff;
    const UNKNOWN_80000: u32 = 0x80000;
    const UNKNOWN_2: u32 = 2;
    (if alive { 0 } else { ALL }) | UNKNOWN_80000 | UNKNOWN_2
}

/// TestSphereB second flags for the capsule (C_human.cpp:5293-5295):
/// COLF2_UNKNOWN_800000 plus COLF2_UNKNOWN_80000 for a non-player, with
/// COLF2_UNKNOWN_800000 0x800000 and COLF2_UNKNOWN_80000 0x80000 per
/// mafia-re 113b5b6 game/collision/g_colls.h:36-37. The caller resolves
/// `is_player` through the mission/game owners (DEFERRED): the original
/// compares `this` against `act_mission->GetGame()->GetPlayer()`.
pub fn test_move_coll_flags2(is_player: bool) -> u32 {
    const UNKNOWN_800000: u32 = 0x800000;
    const UNKNOWN_80000: u32 = 0x80000;
    UNKNOWN_800000 | if is_player { 0 } else { UNKNOWN_80000 }
}

/// Gravity-biased sweep direction (C_human.cpp:5290-5291: `p.dir = dir` then
/// `p.dir.y -= 0.015`).
pub fn test_move_coll_biased_dir(dir: [f32; 3]) -> [f32; 3] {
    [dir[0], dir[1] + TEST_MOVE_COLL_DIR_Y_BIAS, dir[2]]
}

/// Body/head lifts above the legs sphere for the alive path
/// (C_human.cpp:5280-5281): standing (0.55, 1.0), crouching (0.3, 0.6).
/// None on the dead path: the spheres lie along the frame direction there
/// (body = legs + d * 0.5, head = legs - d * 0.5), whose anchoring stays
/// DEFERRED with the frame owner. `crouching` has no owner yet; the caller
/// supplies it.
pub fn test_move_coll_torso_lifts(alive: bool, crouching: bool) -> Option<[f32; 2]> {
    if !alive {
        return None;
    }
    if crouching {
        Some([
            TEST_MOVE_COLL_BODY_LIFT_CROUCH,
            TEST_MOVE_COLL_HEAD_LIFT_CROUCH,
        ])
    } else {
        Some([TEST_MOVE_COLL_BODY_LIFT, TEST_MOVE_COLL_HEAD_LIFT])
    }
}

/// No-hit result (C_human.cpp:5300-5301): `result = from + dir` with the
/// original unbiased displacement.
pub fn test_move_coll_miss_result(from: [f32; 3], dir: [f32; 3]) -> [f32; 3] {
    [from[0] + dir[0], from[1] + dir[1], from[2] + dir[2]]
}

/// Hit result (C_human.cpp:5303): `result = pos_out - (0, 0.4, 0)` once the
/// deferred capsule sweep reports where the legs sphere stops.
pub fn test_move_coll_hit_result(pos_out: [f32; 3]) -> [f32; 3] {
    [
        pos_out[0],
        pos_out[1] - TEST_MOVE_COLL_RESULT_DROP,
        pos_out[2],
    ]
}

/// The guarded prefix assembled from the original leaves in dependency order:
/// radii, flag words, biased sweep direction, torso lifts and the no-hit
/// result. The capsule sweep between the biased direction and the hit result
/// stays DEFERRED (no TestPanak2 owner); frame anchoring of the legs sphere
/// (frame position + TEST_MOVE_COLL_LEGS_LIFT) stays DEFERRED with the frame
/// owner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TestMoveCollProbe {
    pub radii: [f32; 3],
    pub flags: u32,
    pub flags2: u32,
    pub biased_dir: [f32; 3],
    pub torso_lifts: Option<[f32; 2]>,
    pub miss_result: [f32; 3],
}

/// Assemble the guarded prefix (C_human.cpp:5276-5295 plus the :5300-5301
/// no-hit arm). `is_player` and `crouching` are supplied by the caller; their
/// mission/collision owners stay DEFERRED.
pub fn test_move_coll_prefix(
    alive: bool,
    crouching: bool,
    is_player: bool,
    from: [f32; 3],
    dir: [f32; 3],
) -> TestMoveCollProbe {
    TestMoveCollProbe {
        radii: TEST_MOVE_COLL_RADII,
        flags: test_move_coll_flags(alive),
        flags2: test_move_coll_flags2(is_player),
        biased_dir: test_move_coll_biased_dir(dir),
        torso_lifts: test_move_coll_torso_lifts(alive, crouching),
        miss_result: test_move_coll_miss_result(from, dir),
    }
}

/// Legs sphere lift above the local frame position in a vehicle
/// (C_human.cpp:5333: +0.4; GetPos, not the world position).
pub const TEST_MOVE_COLL_OWNER_LEGS_LIFT: f32 = 0.4;
/// Body sphere lift above the owner legs sphere (C_human.cpp:5334: +0.75).
pub const TEST_MOVE_COLL_OWNER_BODY_LIFT: f32 = 0.75;
/// Head sphere lift above the owner legs sphere (C_human.cpp:5335: +1.0).
pub const TEST_MOVE_COLL_OWNER_HEAD_LIFT: f32 = 1.0;
/// Owner capsule radii for legs, body and head (C_human.cpp:5336-5338).
pub const TEST_MOVE_COLL_OWNER_RADII: [f32; 3] = [0.4, 0.35, 0.35];

/// TestMoveCollInOwner flags (C_human.cpp:5341): COLF_UNKNOWN_2 only (no alive
/// arm, no second flags word) per mafia-re 113b5b6 game/collision/g_colls.h:32.
pub fn test_move_coll_owner_flags() -> u32 {
    2
}

/// Owner capsule centres from the local frame position (C_human.cpp:5333-5335:
/// `frame->GetPos()` plus the 0.4/0.75/1.0 lifts). The caller supplies the
/// local frame position; frame anchoring stays DEFERRED with the frame owner.
pub fn test_move_coll_owner_centres(frame_pos: [f32; 3]) -> [[f32; 3]; 3] {
    let legs = [
        frame_pos[0],
        frame_pos[1] + TEST_MOVE_COLL_OWNER_LEGS_LIFT,
        frame_pos[2],
    ];
    let body = [legs[0], legs[1] + TEST_MOVE_COLL_OWNER_BODY_LIFT, legs[2]];
    let head = [legs[0], legs[1] + TEST_MOVE_COLL_OWNER_HEAD_LIFT, legs[2]];
    [legs, body, head]
}

/// TestMoveCollInOwner dispatch gate (C_human.cpp:5343-5351): a railway probes
/// its wagon's dyn_colls, a car probes its dyn_colls only for a seatless human
/// (seat == HUMAN_SEAT_NONE, C_human.cpp:5348); any other carrier (or a seated
/// car human) probes nothing (hit = 0, result = from + dir). `is_car` /
/// `is_railway` resolve `car->type` outside this slice (no car owner here).
pub fn test_move_coll_in_owner_gate(seat: i32, is_car: bool, is_railway: bool) -> bool {
    if is_railway {
        return true;
    }
    is_car && seat == crate::human_actor::HUMAN_SEAT_NONE
}

/// TestMoveCollInOwner result selection (C_human.cpp:5353-5356): shared with
/// TestMoveColl (miss from + unbiased dir, hit pos_out - 0.4 on y).
pub fn test_move_coll_in_owner_result(
    from: [f32; 3],
    dir: [f32; 3],
    hit_pos_out: Option<[f32; 3]>,
) -> [f32; 3] {
    match hit_pos_out {
        Some(pos_out) => test_move_coll_hit_result(pos_out),
        None => test_move_coll_miss_result(from, dir),
    }
}

/// GAME 0x57cfe0 (RVA 0x17cfe0) `C_human::DynSetNextCollOwnerMult` merge
/// contract (mafia-re 113b5b6 game/actors/C_human.cpp:5308-5322, called at
/// :5048 for the `MovementIntern` ground probe and at :5296 for the
/// `TestMoveColl` sweep; :5342 `TestMoveCollInOwner` passes `colls` only and
/// :6298/:6343/:6389 car-door probes build their own one-car lists and stay
/// outside this helper).
/// With staged ghosts the original clears `next_coll_owners` and refills it
/// with `colls` reversed then `ghost_colls` reversed (the two reverse
/// `push_back` loops); without ghosts it passes `colls` straight through.
/// This pure helper returns the merged staging (`None` means "pass `colls`
/// directly", matching the ghost-free arm that leaves `next_coll_owners`
/// untouched); the typed staging itself lives in
/// `human_actor::HumanState::rebuild_next_coll_owners`. Element type stays
/// generic over the opaque dynamic-coll identity so this file assigns no
/// representation to `tDynamicCollObject *`.
/// `is_player` does not enter this merge: it selects `flags2` in
/// [`test_move_coll_prefix`] (C_human.cpp:5293-5295, `this != GetPlayer()`
/// adds `COLF2_UNKNOWN_80000`), while the merged owners ride the
/// `DynSetNextCollOwnerMult` call around the same `TestPanak2` sweep.
pub fn collect_next_coll_owners<T: Clone>(colls: &[T], ghost_colls: &[T]) -> Option<Vec<T>> {
    if ghost_colls.is_empty() {
        return None;
    }
    let mut merged = Vec::with_capacity(colls.len() + ghost_colls.len());
    for coll in colls.iter().rev() {
        merged.push(coll.clone());
    }
    for ghost in ghost_colls.iter().rev() {
        merged.push(ghost.clone());
    }
    Some(merged)
}

/// Whether the `TestMoveColl` sweep threads the merged staging (`true`) or
/// passes `colls` straight through (`false`): exactly `!ghost_colls.is_empty()`
/// (mafia-re 113b5b6 game/actors/C_human.cpp:5310-5322).
pub fn next_coll_owners_use_ghost(ghost_len: usize) -> bool {
    ghost_len != 0
}

// ---------------------------------------------------------------------------
// Movement / MovementIntern control core (C_human.cpp:4752-4795, 5037-5266).
// Pure fixed-point-domain kinematics over caller-supplied probe values; every
// world query and every presentation/animation/sound/particle/police/Death/Hit
// effect stays outside. Action constants live with the HumanState owner
// (human_actor); the local fall/energy/material words are plain values here.
// ---------------------------------------------------------------------------

/// Near-zero bound of the Movement comparisons (mafia-re 113b5b6
/// ls3df/ls3df.h:48 MRG_ZERO 1e-8f; thresholds at C_human.cpp:4757, 4782).
pub const MOVEMENT_MRG_ZERO: f32 = 1e-8;
/// Gravity applied to the fall word (C_human.cpp:5136: `fall += t * 9.81`).
pub const MOVEMENT_GRAVITY: f32 = 9.81;
/// STAND fall seed starting the descent while test_ground holds
/// (C_human.cpp:4760: `fall = .05f`).
pub const MOVEMENT_STAND_FALL_SEED: f32 = 0.05;
/// Fall floor imposed by ground contact or by a ground miss (C_human.cpp:5119
/// -5122, 5124-5126: `if (fall < 3.0f) fall = 3.0f`).
pub const MOVEMENT_FALL_FLOOR: f32 = 3.0;
/// Height gap imposing the fall floor (C_human.cpp:5119: `pos.y - hitpos.y >
/// .4f` takes fall to at least the floor).
pub const MOVEMENT_FALL_FLOOR_DROP: f32 = 0.4;
/// Lethal-fall threshold (C_human.cpp:4766: `if (fall > 50.0f)` drowns the
/// energy word, clears alive and calls Death plus ACTSTATE_NONE; effects
/// DEFERRED, reported as `lethal_fall`).
pub const MOVEMENT_LETHAL_FALL: f32 = 50.0;
/// Void-fall value forced for the player below y -30 while falling fast
/// (C_human.cpp:4763-4765: `fall = 60.0f`; base_cache/position supplied by the
/// caller as `in_void`).
pub const MOVEMENT_VOID_FALL: f32 = 60.0;
/// Void-fall arm thresholds (C_human.cpp:4763-4764: fall > 10, y < -30).
pub const MOVEMENT_VOID_FALL_ARM: f32 = 10.0;
pub const MOVEMENT_VOID_HEIGHT: f32 = -30.0;
/// Ground probe origin lift and sweep depth (C_human.cpp:5065, 5072:
/// `pos + (0, .5, 0)`, delta `(0, -1.5, 0)`).
pub const MOVEMENT_GROUND_LIFT: f32 = 0.5;
pub const MOVEMENT_GROUND_DEPTH: f32 = -1.5;
/// Look-ahead offset along the normalized displacement for the move_ratio>=0.3
/// ground probe (C_human.cpp:5072: `+ d * .3f`).
pub const MOVEMENT_STEP_AHEAD: f32 = 0.3;
/// move_ratio threshold selecting the look-ahead probe (C_human.cpp:5064).
pub const MOVEMENT_RATIO_THRESHOLD: f32 = 0.3;
/// Ground probe flag words (C_human.cpp:5065-5067, 5072-5074:
/// COLF_UNKNOWN_80000 | COLF_UNKNOWN_2, COLF2_UNKNOWN_800000).
pub const MOVEMENT_GROUND_FLAGS: u32 = 0x80002;
pub const MOVEMENT_GROUND_FLAGS2: u32 = 0x800000;
/// Slope reject: ground angles above this from up are treated as no ground
/// (C_human.cpp:5077: `AngleTo(up) > 1.0f`, radians).
pub const MOVEMENT_SLOPE_LIMIT_RAD: f32 = 1.0;
/// Ground-contact epsilon (C_human.cpp:5113: |hitpos.y^2 - pos.y^2| < .09 keeps
/// may_fall false; .09 is the exact literal, not a squared 0.3).
pub const MOVEMENT_GROUND_TOUCH_EPSILON: f32 = 0.09;
/// Hard-landing threshold (C_human.cpp:5186: `if (fall > 5.5f)` zeroes the
/// velocity words and may call Hit; effects DEFERRED).
pub const MOVEMENT_HARD_LANDING_FALL: f32 = 5.5;
/// Small-step boost (C_human.cpp:5140-5143): STAND/FALL steps with Square() <
/// .01 normalize to length .1.
pub const MOVEMENT_MIN_STEP_SQUARED: f32 = 0.01;
pub const MOVEMENT_MIN_STEP_LENGTH: f32 = 0.1;
/// Water-death and death-fall materials (mafia-re 113b5b6
/// game/actors/g_material.h:15-16; read at C_human.cpp:5081, 5108, 5149).
pub const MAT_WATER_DEATH: u32 = 0x1f;
pub const MAT_DEATH: u32 = 0x28;
/// Ground coll flags selecting test_ground (C_human.cpp:5118:
/// COLLHF_UNKNOWN_40 / COLLHF_UNKNOWN_10000; values from the flag names).
pub const COLLHF_UNKNOWN_40: u32 = 0x40;
pub const COLLHF_UNKNOWN_10000: u32 = 0x10000;
/// move_coll owner arm flag (C_human.cpp:5232: `move_coll->flags & 0x40`).
pub const MOVE_COLL_OWNER_FLAG: u32 = 0x40;
/// move_ratio sampling periods in ms (C_human.cpp:4779: 100 player, 500 other).
pub const MOVE_RATIO_PERIOD_PLAYER_MS: u32 = 100;
pub const MOVE_RATIO_PERIOD_OTHER_MS: u32 = 500;

/// Ground TestLineV probe shape for MovementIntern (C_human.cpp:5064-5074).
/// The caller runs the sweep; the origin/delta/flag words are owned here.
/// `step_actor` (set to this around the look-ahead probe at :5071/:5076, then
/// cleared) has no collision-manager owner and stays DEFERRED.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovementGroundQuery {
    pub origin: [f32; 3],
    pub delta: [f32; 3],
    pub flags: u32,
    pub flags2: u32,
}

/// Assemble the ground probe (C_human.cpp:5064-5075): base origin
/// pos + (0, 0.5, 0) sweeping (0, -1.5, 0); with move_ratio >= 0.3 the origin
/// advances 0.3 along the normalized displacement. `normalized_dir` carries the
/// :5069-5070 normalization (the original only reaches it with a non-null move
/// per the :5047 gate; a missing direction falls back to the base origin as a
/// defensive refusal, not original behavior).
pub fn movement_ground_query(
    pos: [f32; 3],
    move_ratio: f32,
    normalized_dir: Option<[f32; 3]>,
) -> MovementGroundQuery {
    let mut origin = [pos[0], pos[1] + MOVEMENT_GROUND_LIFT, pos[2]];
    if move_ratio >= MOVEMENT_RATIO_THRESHOLD {
        if let Some(dir) = normalized_dir {
            origin = [
                origin[0] + dir[0] * MOVEMENT_STEP_AHEAD,
                origin[1] + dir[1] * MOVEMENT_STEP_AHEAD,
                origin[2] + dir[2] * MOVEMENT_STEP_AHEAD,
            ];
        }
    }
    MovementGroundQuery {
        origin,
        delta: [0.0, MOVEMENT_GROUND_DEPTH, 0.0],
        flags: MOVEMENT_GROUND_FLAGS,
        flags2: MOVEMENT_GROUND_FLAGS2,
    }
}

/// Caller-supplied ground contact (TestLineV hit): hit position, ground normal,
/// collision material and header flags. A slope above MOVEMENT_SLOPE_LIMIT_RAD
/// from up must be filtered to None by the caller before this core runs
/// (C_human.cpp:5077-5078 `coll = NULL`; the angle itself is a world-query
/// concern only in that the caller computes it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundContact {
    pub hitpos: [f32; 3],
    pub ground_normal: [f32; 3],
    pub material: u32,
    pub header_flags: u32,
}

/// Ground-contact update (C_human.cpp:5080-5127): water-death drowning, ground
/// material capture, player death-fall, may_fall touch epsilon, test_ground
/// latch and the 0.4 fall floor. `in_car` selects the :5115-5118 arms
/// (test_ground = false in a car). Camera/particle/sound/indicator/Death side
/// effects are DEFERRED; drowning and the player death-fall ride outcome flags.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundResolution {
    pub ground_normal: [f32; 3],
    pub ground_material: u32,
    pub test_ground: bool,
    pub fall: f32,
    pub may_fall: bool,
    pub drowned_now: bool,
    pub death_fall: bool,
}

pub fn movement_apply_ground(
    contact: Option<GroundContact>,
    pos: [f32; 3],
    fall: f32,
    in_car: bool,
    is_player: bool,
) -> GroundResolution {
    let Some(contact) = contact else {
        // C_human.cpp:5124-5126: a ground miss latches test_ground and takes
        // fall to at least the floor (plain comparison: NaN keeps NaN, as the
        // original's `if (fall < 3.0f)` does).
        return GroundResolution {
            ground_normal: [0.0, 1.0, 0.0],
            ground_material: 0,
            test_ground: true,
            fall: if fall < MOVEMENT_FALL_FLOOR {
                MOVEMENT_FALL_FLOOR
            } else {
                fall
            },
            may_fall: true,
            drowned_now: false,
            death_fall: false,
        };
    };
    let drowned_now = contact.material == MAT_WATER_DEATH;
    let death_fall = is_player && contact.material == MAT_DEATH;
    // NaN-preserving native compare: NaN must yield `may_fall == true`.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    let may_fall =
        !(contact.hitpos[1] * contact.hitpos[1] - pos[1] * pos[1] < MOVEMENT_GROUND_TOUCH_EPSILON);
    let test_ground = if in_car {
        false
    } else {
        contact.header_flags & COLLHF_UNKNOWN_40 != 0
            || contact.header_flags & COLLHF_UNKNOWN_10000 != 0
    };
    let fall = if pos[1] - contact.hitpos[1] > MOVEMENT_FALL_FLOOR_DROP {
        // Plain comparison (NaN keeps NaN, as the original's `< 3.0f` does).
        if fall < MOVEMENT_FALL_FLOOR {
            MOVEMENT_FALL_FLOOR
        } else {
            fall
        }
    } else {
        fall
    };
    GroundResolution {
        ground_normal: contact.ground_normal,
        ground_material: contact.material,
        test_ground,
        fall,
        may_fall,
        drowned_now,
        death_fall,
    }
}

fn movement_dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn movement_sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn movement_cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Slope projection (C_human.cpp:5128-5133): unless jumping and the
/// displacement is non-null, re-express it tangentially to the ground normal
/// keeping its length (`move = ground_normal.Cross(move.Cross(ground_normal))`
///, then normalize and scale by the original magnitude). A zero-length
/// projection maps to zero (the original Normalize-then-scale order would
/// divide by zero there; the guard is defensive, not original behavior).
pub fn movement_project_on_ground(displacement: [f32; 3], ground_normal: [f32; 3]) -> [f32; 3] {
    let length_squared = movement_dot(displacement, displacement);
    if length_squared <= 0.0 {
        return [0.0, 0.0, 0.0];
    }
    let length = length_squared.sqrt();
    let tangential = movement_cross(ground_normal, movement_cross(displacement, ground_normal));
    let tangential_length_squared = movement_dot(tangential, tangential);
    if tangential_length_squared <= 0.0 {
        return [0.0, 0.0, 0.0];
    }
    let tangential_length = tangential_length_squared.sqrt();
    [
        tangential[0] / tangential_length * length,
        tangential[1] / tangential_length * length,
        tangential[2] / tangential_length * length,
    ]
}

/// Gravity step (C_human.cpp:5136-5137): `fall += t * 9.81; move.y -= fall * t`
/// with t = time_ms * 0.001 (C_human.cpp:5039).
pub fn movement_apply_gravity(displacement: [f32; 3], fall: f32, time_ms: u32) -> ([f32; 3], f32) {
    let t = time_ms as f32 * 0.001;
    let fall = fall + t * MOVEMENT_GRAVITY;
    (
        [displacement[0], displacement[1] - fall * t, displacement[2]],
        fall,
    )
}

/// Small-step boost (C_human.cpp:5140-5143): STAND/FALL steps with Square() <
/// 0.01 normalize to length 0.1. Other actions pass through untouched.
pub fn movement_boost_small_step(action_state: i32, step: [f32; 3]) -> [f32; 3] {
    if (action_state == HUMAN_ACTION_FALL || action_state == HUMAN_ACTION_STAND)
        && movement_dot(step, step) < MOVEMENT_MIN_STEP_SQUARED
    {
        let length = movement_dot(step, step).sqrt();
        if length > 0.0 {
            return [
                step[0] / length * MOVEMENT_MIN_STEP_LENGTH,
                step[1] / length * MOVEMENT_MIN_STEP_LENGTH,
                step[2] / length * MOVEMENT_MIN_STEP_LENGTH,
            ];
        }
    }
    step
}

/// Caller-supplied step contact (TestMoveColl/TestMoveCollInOwner hit): the
/// swept rest position, the hit collision material and whether the
/// COLLHF 0x40 owner arm applies (C_human.cpp:5232; the Bottle/Grenade/Car/
/// Bridge/Entity Collision/Event calls stay DEFERRED with the actor owners).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepContact {
    pub newpos: [f32; 3],
    pub material: u32,
    pub owner_arm: bool,
}

/// Step resolution (C_human.cpp:5149-5256 pins the owned words): water-death
/// drowning, FALL landing (STAND on landing, velocity zeroing above 5.5 fall,
/// landing-damage value, PersonAnim request flag; particles/sound/Hit effects
/// DEFERRED), STAND x/z pinning, fall reset and JUMP->STAND. `ground_present`
/// is whether the ground probe hit (C_human.cpp:5175 `landed = coll != NULL`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepResolution {
    pub pos: [f32; 3],
    pub fall: f32,
    pub action_state: i32,
    pub landed: bool,
    pub zeroed_velocity: bool,
    pub landing_damage: Option<f32>,
    pub wants_person_anim: bool,
    /// Hard landing below the damage bar (C_human.cpp:5196-5199): the original
    /// selects ANIM_DOSKOK plus SetWaitAnim here; the animation effect stays
    /// DEFERRED with the animation owner.
    pub selects_doskok: bool,
    pub drowned_now: bool,
}

pub fn movement_apply_step(
    action_state: i32,
    pos: [f32; 3],
    displacement: [f32; 3],
    fall: f32,
    jump_fall: bool,
    alive: bool,
    ground_present: bool,
    contact: Option<StepContact>,
) -> StepResolution {
    let Some(contact) = contact else {
        return StepResolution {
            pos: [
                pos[0] + displacement[0],
                pos[1] + displacement[1],
                pos[2] + displacement[2],
            ],
            fall,
            action_state,
            landed: false,
            zeroed_velocity: false,
            landing_damage: None,
            wants_person_anim: false,
            selects_doskok: false,
            drowned_now: false,
        };
    };
    if contact.material == MAT_WATER_DEATH {
        // C_human.cpp:5149-5173: drowning clears the step contact, and the
        // `if (!move_coll) pos += move` arm then applies the full
        // displacement; the drowning effects (camera/particles/sound/energy)
        // stay DEFERRED behind `drowned_now`.
        return StepResolution {
            pos: [
                pos[0] + displacement[0],
                pos[1] + displacement[1],
                pos[2] + displacement[2],
            ],
            fall,
            action_state,
            landed: false,
            zeroed_velocity: false,
            landing_damage: None,
            wants_person_anim: false,
            selects_doskok: false,
            drowned_now: true,
        };
    }
    let mut landed = ground_present;
    if action_state == HUMAN_ACTION_FALL && !landed {
        let length = movement_dot(displacement, displacement).sqrt();
        if length > MOVEMENT_MIN_STEP_LENGTH {
            let travelled = movement_sub(contact.newpos, pos);
            let travelled_len = movement_dot(travelled, travelled).sqrt();
            if travelled_len / length < 0.2 {
                landed = true;
            }
        } else {
            landed = true;
        }
    }
    // Pinning below keys on the pre-landing action (C_human.cpp:5223: the
    // `else if STAND` arm only runs when the FALL-landing arm did not).
    let pin_stand = action_state == HUMAN_ACTION_STAND;
    let mut action_state = action_state;
    let mut fall = fall;
    let mut zeroed_velocity = false;
    let mut landing_damage = None;
    let mut wants_person_anim = false;
    let mut selects_doskok = false;
    if action_state == HUMAN_ACTION_FALL && landed {
        action_state = HUMAN_ACTION_STAND;
        if fall > MOVEMENT_HARD_LANDING_FALL {
            zeroed_velocity = true;
            if alive {
                wants_person_anim = true;
            }
            if ((if jump_fall { 18 } else { 8 }) as f32) < fall {
                landing_damage = Some((fall - (if jump_fall { 12.0 } else { 2.0 })) * 10.0);
            } else {
                // Below the damage bar the original selects ANIM_DOSKOK plus
                // SetWaitAnim instead of Hit (C_human.cpp:5196-5199); the
                // :5190 breath PersonAnim above still stands.
                selects_doskok = true;
            }
        } else if alive {
            wants_person_anim = true;
        }
    } else if action_state == HUMAN_ACTION_STAND {
        // C_human.cpp:5223-5226: the stand pins x/z, keeping only the swept y.
        // No vector work happens here; the pin below applies it.
    }
    let mut newpos = contact.newpos;
    if pin_stand {
        newpos[0] = pos[0];
        newpos[2] = pos[2];
    }
    if action_state != HUMAN_ACTION_FALL || landed {
        fall = 0.0;
    }
    if action_state == HUMAN_ACTION_JUMP {
        action_state = HUMAN_ACTION_STAND;
    }
    StepResolution {
        pos: newpos,
        fall,
        action_state,
        landed,
        zeroed_velocity,
        landing_damage,
        wants_person_anim,
        selects_doskok,
        drowned_now: false,
    }
}

/// Movement dispatch (C_human.cpp:4752-4795, MovementIntern call :4776 plus the
/// 4778-4788 move_ratio sampling): STAND seeds fall 0.05 while test_ground
/// holds and otherwise runs MovementIntern; FALL applies the void-fall and
/// lethal-fall arms; WALK/JUMP run MovementIntern; STEPS/AGONY/DEAD arms stay
/// DEFERRED (reported as `deferred_arm`, MovementIntern not run).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovementDispatch {
    pub run_intern: bool,
    pub fall: f32,
    pub lethal_fall: bool,
    pub deferred_arm: bool,
}

pub fn movement_dispatch(
    action_state: i32,
    fall: f32,
    test_ground: bool,
    in_void: bool,
) -> MovementDispatch {
    match action_state {
        HUMAN_ACTION_STAND => {
            if fall <= MOVEMENT_MRG_ZERO {
                if !test_ground {
                    return MovementDispatch {
                        run_intern: false,
                        fall,
                        lethal_fall: false,
                        deferred_arm: false,
                    };
                }
                return MovementDispatch {
                    run_intern: true,
                    fall: MOVEMENT_STAND_FALL_SEED,
                    lethal_fall: false,
                    deferred_arm: false,
                };
            }
            MovementDispatch {
                run_intern: true,
                fall,
                lethal_fall: false,
                deferred_arm: false,
            }
        }
        HUMAN_ACTION_FALL => {
            let fall = if in_void { MOVEMENT_VOID_FALL } else { fall };
            if fall > MOVEMENT_LETHAL_FALL {
                return MovementDispatch {
                    run_intern: false,
                    fall,
                    lethal_fall: true,
                    deferred_arm: false,
                };
            }
            MovementDispatch {
                run_intern: true,
                fall,
                lethal_fall: false,
                deferred_arm: false,
            }
        }
        HUMAN_ACTION_WALK | HUMAN_ACTION_JUMP => MovementDispatch {
            run_intern: true,
            fall,
            lethal_fall: false,
            deferred_arm: false,
        },
        _ => MovementDispatch {
            run_intern: false,
            fall,
            lethal_fall: false,
            deferred_arm: true,
        },
    }
}

/// move_ratio sampling (C_human.cpp:4778-4788): every 100 ms for the player,
/// 500 ms for the others, `move_ratio = expected ~ 0 ? 1 : (travelled /
/// expected + move_ratio) * 0.5`, then move_pos = pos and move_time = 0.
/// Otherwise move_time accumulates. All lengths use f32 Magnitude semantics.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveRatioState {
    pub move_ratio: f32,
    pub move_time_ms: u32,
    pub move_pos: [f32; 3],
}

pub fn movement_update_ratio(
    state: MoveRatioState,
    pos: [f32; 3],
    velocity_magnitude: f32,
    elapsed_ms: u32,
    is_player: bool,
) -> MoveRatioState {
    let period = if is_player {
        MOVE_RATIO_PERIOD_PLAYER_MS
    } else {
        MOVE_RATIO_PERIOD_OTHER_MS
    };
    let move_time_ms = state.move_time_ms + elapsed_ms;
    if move_time_ms < period {
        return MoveRatioState {
            move_time_ms,
            ..state
        };
    }
    let travelled = movement_sub(pos, state.move_pos);
    let travelled_len = movement_dot(travelled, travelled).sqrt();
    let expected = move_time_ms as f32 * velocity_magnitude * 0.001;
    let move_ratio = if expected.abs() < MOVEMENT_MRG_ZERO {
        1.0
    } else {
        (travelled_len / expected + state.move_ratio) * 0.5
    };
    MoveRatioState {
        move_ratio,
        move_time_ms: 0,
        move_pos: pos,
    }
}

/// FALL entry at the tail of MovementIntern (C_human.cpp:5258-5265): with
/// may_fall set and fall above 2.0 outside FALL, JUMP keeps falling (may_fall
/// false below fall 10) while any other action enters FALL (recording whether
/// it jumped), selecting the PersonAnim handoff for the animation owner.
/// The caller applies `jump_fall` to the latch only when the action changes to
/// FALL (the original writes it solely on that path, :5261).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FallEntry {
    pub action_state: i32,
    pub jump_fall: bool,
    pub wants_person_anim: bool,
}

pub fn movement_fall_entry(action_state: i32, fall: f32, may_fall: bool) -> FallEntry {
    let mut may_fall = may_fall;
    if may_fall && action_state == HUMAN_ACTION_JUMP && fall < 10.0 {
        may_fall = false;
    }
    if may_fall && fall > 2.0 && action_state != HUMAN_ACTION_FALL {
        return FallEntry {
            action_state: HUMAN_ACTION_FALL,
            jump_fall: action_state == HUMAN_ACTION_JUMP,
            wants_person_anim: true,
        };
    }
    FallEntry {
        action_state,
        jump_fall: false,
        wants_person_anim: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capsule_radii_and_lifts_match_body_constants() {
        // Hand-derived from C_human::TestMoveColl (mafia-re 113b5b6
        // game/actors/C_human.cpp:5278-5288): legs lift 0.405, radii
        // 0.4/0.25/0.35, alive lifts (0.55, 1.0) standing and (0.3, 0.6)
        // crouching.
        assert_eq!(TEST_MOVE_COLL_RADII, [0.4, 0.25, 0.35]);
        assert_eq!(TEST_MOVE_COLL_LEGS_LIFT, 0.405);
        assert_eq!(TEST_MOVE_COLL_RESULT_DROP, 0.4);
        assert_eq!(TEST_MOVE_COLL_DIR_Y_BIAS, -0.015);
        assert_eq!(test_move_coll_torso_lifts(true, false), Some([0.55, 1.0]));
        assert_eq!(test_move_coll_torso_lifts(true, true), Some([0.3, 0.6]));
        assert_eq!(test_move_coll_torso_lifts(false, false), None);
        assert_eq!(test_move_coll_torso_lifts(false, true), None);
    }

    #[test]
    fn flag_words_follow_alive_and_player_arms() {
        // Hand-derived from C_human.cpp:5292-5295 with g_colls.h:31-37:
        // alive 0x80002, dead 0xffffffff; player second flags 0x800000,
        // non-player 0x880000.
        assert_eq!(test_move_coll_flags(true), 0x80002);
        assert_eq!(test_move_coll_flags(false), 0xffff_ffff);
        assert_eq!(test_move_coll_flags2(true), 0x800000);
        assert_eq!(test_move_coll_flags2(false), 0x880000);
    }

    #[test]
    fn sweep_bias_touches_only_y_and_miss_keeps_original_dir() {
        // Hand-derived from C_human.cpp:5290-5291 (:5300-5301 for the miss
        // arm): the capsule sweeps dir with y biased by -0.015 while the
        // no-hit result adds the original unbiased displacement.
        assert_eq!(
            test_move_coll_biased_dir([1.0, 0.0, -2.0]),
            [1.0, -0.015, -2.0]
        );
        assert_eq!(
            test_move_coll_miss_result([1.0, 2.0, 3.0], [0.5, -0.25, -1.0]),
            [1.5, 1.75, 2.0]
        );
    }

    #[test]
    fn hit_result_drops_stopped_legs_sphere_by_legs_radius() {
        // Hand-derived from C_human.cpp:5303 (pos_out - 0.4 on y only).
        // Both vectors are bit-exact in f32 (verified by derivation).
        assert_eq!(
            test_move_coll_hit_result([10.0, 5.0, -3.0]),
            [10.0, 4.6, -3.0]
        );
        assert_eq!(test_move_coll_hit_result([0.0, 0.0, 0.0]), [0.0, -0.4, 0.0]);
    }

    #[test]
    fn prefix_assembles_original_order_without_descent() {
        // The guarded prefix for an alive non-player (C_human.cpp:5276-5295
        // plus the :5300-5301 no-hit arm); the capsule sweep itself stays
        // DEFERRED with no owner.
        let probe = test_move_coll_prefix(true, false, false, [1.0, 2.0, 3.0], [0.0, 0.0, 0.0]);
        assert_eq!(
            probe,
            TestMoveCollProbe {
                radii: [0.4, 0.25, 0.35],
                flags: 0x80002,
                flags2: 0x880000,
                biased_dir: [0.0, -0.015, 0.0],
                torso_lifts: Some([0.55, 1.0]),
                miss_result: [1.0, 2.0, 3.0],
            }
        );
        // Dead player: all-bits flags, player second flags, no torso lifts
        // (frame-dir anchored, deferred), miss result still from + dir.
        let probe = test_move_coll_prefix(false, false, true, [1.0, 2.0, 3.0], [0.5, -0.25, -1.0]);
        assert_eq!(probe.flags, 0xffff_ffff);
        assert_eq!(probe.flags2, 0x800000);
        assert_eq!(probe.torso_lifts, None);
        assert_eq!(probe.biased_dir, [0.5, -0.265, -1.0]);
        assert_eq!(probe.miss_result, [1.5, 1.75, 2.0]);
    }

    #[test]
    fn owner_centres_radii_flags_gate_and_result_match_body() {
        // Hand-derived from C_human::TestMoveCollInOwner (mafia-re 113b5b6
        // game/actors/C_human.cpp:5333-5356): local-frame lifts 0.4/0.75/1.0,
        // radii 0.4/0.35/0.35, COLF_UNKNOWN_2-only flags, the seat dispatch
        // gate and the shared hit/miss selection. Expected centres reuse the
        // lift constants by construction (f32 lift sums are not hand-pinned).
        assert_eq!(TEST_MOVE_COLL_OWNER_RADII, [0.4, 0.35, 0.35]);
        assert_eq!(test_move_coll_owner_flags(), 2);
        let [legs, body, head] = test_move_coll_owner_centres([1.0, 2.0, 3.0]);
        assert_eq!(legs, [1.0, 2.0 + TEST_MOVE_COLL_OWNER_LEGS_LIFT, 3.0]);
        assert_eq!(
            body,
            [
                1.0,
                2.0 + TEST_MOVE_COLL_OWNER_LEGS_LIFT + TEST_MOVE_COLL_OWNER_BODY_LIFT,
                3.0
            ]
        );
        assert_eq!(
            head,
            [
                1.0,
                2.0 + TEST_MOVE_COLL_OWNER_LEGS_LIFT + TEST_MOVE_COLL_OWNER_HEAD_LIFT,
                3.0
            ]
        );
        // Railway probes regardless of seat; a car only for the seatless
        // human (seat == HUMAN_SEAT_NONE, :5348); other carriers probe nothing.
        assert!(test_move_coll_in_owner_gate(-1, true, false));
        assert!(test_move_coll_in_owner_gate(0, false, true));
        assert!(!test_move_coll_in_owner_gate(0, true, false));
        assert!(!test_move_coll_in_owner_gate(-1, false, false));
        assert_eq!(
            test_move_coll_in_owner_result([1.0, 2.0, 3.0], [0.5, -0.25, -1.0], None),
            [1.5, 1.75, 2.0]
        );
        assert_eq!(
            test_move_coll_in_owner_result(
                [1.0, 2.0, 3.0],
                [0.5, -0.25, -1.0],
                Some([10.0, 5.0, -3.0])
            ),
            test_move_coll_hit_result([10.0, 5.0, -3.0])
        );
    }

    #[test]
    fn ground_query_offsets_match_probe_arms() {
        // Hand-derived from C_human.cpp:5064-5075: base origin pos + (0, 0.5,
        // 0) sweeping (0, -1.5, 0) with 0x80002/0x800000 flag words; the
        // move_ratio >= 0.3 arm advances the origin 0.3 along the normalized
        // displacement. Expected origins reuse the lift constants by
        // construction.
        let base = movement_ground_query([1.0, 2.0, 3.0], 0.0, None);
        assert_eq!(
            base,
            MovementGroundQuery {
                origin: [1.0, 2.0 + MOVEMENT_GROUND_LIFT, 3.0],
                delta: [0.0, MOVEMENT_GROUND_DEPTH, 0.0],
                flags: 0x80002,
                flags2: 0x800000,
            }
        );
        let ahead = movement_ground_query([1.0, 2.0, 3.0], 0.5, Some([1.0, 0.0, 0.0]));
        assert_eq!(
            ahead.origin,
            [1.0 + MOVEMENT_STEP_AHEAD, 2.0 + MOVEMENT_GROUND_LIFT, 3.0]
        );
        assert_eq!(ahead.delta, [0.0, -1.5, 0.0]);
        // Below the threshold the look-ahead is skipped even with a direction.
        let below = movement_ground_query([1.0, 2.0, 3.0], 0.2, Some([1.0, 0.0, 0.0]));
        assert_eq!(below.origin, base.origin);
    }

    #[test]
    fn ground_contact_updates_words_per_arms() {
        // Hand-derived from C_human.cpp:5080-5127. A miss keeps falling
        // (test_ground true, fall floored at 3.0, up normal, no material).
        let miss = movement_apply_ground(None, [0.0, 5.0, 0.0], 0.0, false, false);
        assert!(miss.test_ground);
        assert_eq!(miss.fall, 3.0);
        assert_eq!(miss.ground_normal, [0.0, 1.0, 0.0]);
        assert_eq!(miss.ground_material, 0);
        assert!(miss.may_fall);
        // Touch contact (hitpos.y^2 - pos.y^2 = 0 < .09): may_fall false,
        // header 0x40 latches test_ground, small gap leaves fall alone.
        let touch = movement_apply_ground(
            Some(GroundContact {
                hitpos: [0.0, 5.0, 0.0],
                ground_normal: [0.0, 1.0, 0.0],
                material: 7,
                header_flags: COLLHF_UNKNOWN_40,
            }),
            [0.0, 5.0, 0.0],
            1.0,
            false,
            false,
        );
        assert!(!touch.may_fall);
        assert!(touch.test_ground);
        assert_eq!(touch.ground_material, 7);
        assert_eq!(touch.fall, 1.0);
        assert!(!touch.drowned_now);
        assert!(!touch.death_fall);
        // Raised contact (36 - 25 >= .09): may_fall true; the contact sits
        // above pos so the 0.4 gap floor does not apply; in-car clears
        // test_ground; water drowns; MAT_DEATH only fails the player's fall.
        let raised = movement_apply_ground(
            Some(GroundContact {
                hitpos: [0.0, 6.0, 0.0],
                ground_normal: [0.0, 1.0, 0.0],
                material: 7,
                header_flags: 0,
            }),
            [0.0, 5.0, 0.0],
            1.0,
            true,
            false,
        );
        assert!(raised.may_fall);
        assert!(!raised.test_ground);
        assert_eq!(raised.fall, 1.0);
        // Gap contact (pos 0.5 above hit): may_fall false, the > 0.4 gap
        // floors fall to 3.0 (C_human.cpp:5119-5122).
        let gap = movement_apply_ground(
            Some(GroundContact {
                hitpos: [0.0, 4.5, 0.0],
                ground_normal: [0.0, 1.0, 0.0],
                material: 7,
                header_flags: COLLHF_UNKNOWN_40,
            }),
            [0.0, 5.0, 0.0],
            1.0,
            false,
            false,
        );
        assert!(!gap.may_fall);
        assert!(gap.test_ground);
        assert_eq!(gap.fall, 3.0);
        let water = movement_apply_ground(
            Some(GroundContact {
                hitpos: [0.0, 5.0, 0.0],
                ground_normal: [0.0, 1.0, 0.0],
                material: MAT_WATER_DEATH,
                header_flags: 0,
            }),
            [0.0, 5.0, 0.0],
            1.0,
            false,
            false,
        );
        assert!(water.drowned_now);
        assert_eq!(water.ground_material, MAT_WATER_DEATH);
        let death = movement_apply_ground(
            Some(GroundContact {
                hitpos: [0.0, 5.0, 0.0],
                ground_normal: [0.0, 1.0, 0.0],
                material: MAT_DEATH,
                header_flags: COLLHF_UNKNOWN_10000,
            }),
            [0.0, 5.0, 0.0],
            1.0,
            false,
            true,
        );
        assert!(death.death_fall);
        assert!(death.test_ground);
        let death_npc = movement_apply_ground(
            Some(GroundContact {
                hitpos: [0.0, 5.0, 0.0],
                ground_normal: [0.0, 1.0, 0.0],
                material: MAT_DEATH,
                header_flags: 0,
            }),
            [0.0, 5.0, 0.0],
            1.0,
            false,
            false,
        );
        assert!(!death_npc.death_fall);
    }

    #[test]
    fn slope_projection_gravity_and_boost_follow_body() {
        // Hand-derived from C_human.cpp:5128-5133 (tangential projection
        // keeping length), :5136-5137 (fall += t * 9.81, move.y -= fall * t)
        // and :5140-5143 (STAND/FALL steps below .01 squared normalize to .1).
        assert_eq!(
            movement_project_on_ground([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            [1.0, 0.0, 0.0]
        );
        assert_eq!(
            movement_project_on_ground([0.0, -2.0, 0.0], [0.0, 1.0, 0.0]),
            [0.0, 0.0, 0.0]
        );
        assert_eq!(
            movement_project_on_ground([0.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            [0.0, 0.0, 0.0]
        );
        let (moved, fall) = movement_apply_gravity([1.0, 2.0, 3.0], 0.0, 500);
        assert_eq!(fall, 0.5 * MOVEMENT_GRAVITY);
        assert_eq!(moved[0], 1.0);
        assert_eq!(moved[2], 3.0);
        assert_eq!(moved[1], 2.0 - fall * 0.5);
        let (moved, fall) = movement_apply_gravity([0.0, 0.0, 0.0], 1.0, 0);
        assert_eq!((moved, fall), ([0.0, 0.0, 0.0], 1.0));
        assert_eq!(
            movement_boost_small_step(HUMAN_ACTION_STAND, [0.05, 0.0, 0.0]),
            [0.1, 0.0, 0.0]
        );
        assert_eq!(
            movement_boost_small_step(HUMAN_ACTION_FALL, [0.05, 0.0, 0.0]),
            [0.1, 0.0, 0.0]
        );
        assert_eq!(
            movement_boost_small_step(HUMAN_ACTION_JUMP, [0.05, 0.0, 0.0]),
            [0.05, 0.0, 0.0]
        );
        assert_eq!(
            movement_boost_small_step(HUMAN_ACTION_STAND, [1.0, 0.0, 0.0]),
            [1.0, 0.0, 0.0]
        );
    }

    #[test]
    fn dispatch_covers_stand_fall_walk_jump_and_defers_rest() {
        // Hand-derived from C_human.cpp:4754-4795: STAND seeds 0.05 while
        // test_ground holds and idles without it; FALL applies the void and
        // lethal arms; WALK/JUMP run; other actions (e.g. 6 = STEPS) defer.
        let idle = movement_dispatch(HUMAN_ACTION_STAND, 0.0, false, false);
        assert!(!idle.run_intern);
        assert_eq!(idle.fall, 0.0);
        let seed = movement_dispatch(HUMAN_ACTION_STAND, 0.0, true, false);
        assert!(seed.run_intern);
        assert_eq!(seed.fall, MOVEMENT_STAND_FALL_SEED);
        let falling = movement_dispatch(HUMAN_ACTION_STAND, 1.0, true, false);
        assert!(falling.run_intern);
        assert_eq!(falling.fall, 1.0);
        let lethal = movement_dispatch(HUMAN_ACTION_FALL, 51.0, true, false);
        assert!(lethal.lethal_fall);
        assert!(!lethal.run_intern);
        let voided = movement_dispatch(HUMAN_ACTION_FALL, 11.0, true, true);
        assert_eq!(voided.fall, MOVEMENT_VOID_FALL);
        assert!(voided.lethal_fall);
        let walking = movement_dispatch(HUMAN_ACTION_WALK, 0.0, false, false);
        assert!(walking.run_intern);
        assert!(!walking.deferred_arm);
        let jumping = movement_dispatch(HUMAN_ACTION_JUMP, 4.0, false, false);
        assert!(jumping.run_intern);
        assert_eq!(jumping.fall, 4.0);
        let steps = movement_dispatch(6, 0.0, true, false);
        assert!(!steps.run_intern);
        assert!(steps.deferred_arm);
    }

    #[test]
    fn step_resolution_covers_miss_water_land_and_stand_pin() {
        // Hand-derived from C_human.cpp:5149-5256. A miss applies the full
        // displacement; step water drowns yet still applies it (:5172); a
        // STAND hit pins x/z keeping the swept y and resets fall; a hard
        // FALL landing (fall 10, non-jump: 8 < 10) stands with zeroed velocity
        // and (10 - 2) * 10 damage; a light landing requests the breath anim.
        let miss = movement_apply_step(
            HUMAN_ACTION_STAND,
            [1.0, 2.0, 3.0],
            [0.5, -0.25, -1.0],
            4.0,
            false,
            true,
            true,
            None,
        );
        assert_eq!(miss.pos, [1.5, 1.75, 2.0]);
        assert_eq!(miss.fall, 4.0);
        assert!(!miss.landed);
        let water = movement_apply_step(
            HUMAN_ACTION_STAND,
            [1.0, 2.0, 3.0],
            [0.5, -0.25, -1.0],
            4.0,
            false,
            true,
            true,
            Some(StepContact {
                newpos: [9.0, 9.0, 9.0],
                material: MAT_WATER_DEATH,
                owner_arm: false,
            }),
        );
        assert!(water.drowned_now);
        assert_eq!(water.pos, [1.5, 1.75, 2.0]);
        let stand = movement_apply_step(
            HUMAN_ACTION_STAND,
            [1.0, 2.0, 3.0],
            [0.0, -1.0, 0.0],
            4.0,
            false,
            true,
            true,
            Some(StepContact {
                newpos: [7.0, 1.5, 8.0],
                material: 3,
                owner_arm: false,
            }),
        );
        assert_eq!(stand.pos, [1.0, 1.5, 3.0]);
        assert_eq!(stand.fall, 0.0);
        assert_eq!(stand.action_state, HUMAN_ACTION_STAND);
        assert!(stand.landed);
        let hard = movement_apply_step(
            HUMAN_ACTION_FALL,
            [1.0, 5.0, 3.0],
            [0.0, -4.0, 0.0],
            10.0,
            false,
            true,
            true,
            Some(StepContact {
                newpos: [1.0, 1.0, 3.0],
                material: 3,
                owner_arm: false,
            }),
        );
        assert_eq!(hard.action_state, HUMAN_ACTION_STAND);
        assert!(hard.landed);
        assert!(hard.zeroed_velocity);
        assert_eq!(hard.landing_damage, Some(80.0));
        assert!(hard.wants_person_anim);
        assert!(!hard.selects_doskok);
        assert_eq!(hard.fall, 0.0);
        // Jump-origin hard landing discounts 12 up to the 18 bar:
        // (20 - 12) * 10, exact in f32.
        let jump_hard = movement_apply_step(
            HUMAN_ACTION_FALL,
            [1.0, 5.0, 3.0],
            [0.0, -4.0, 0.0],
            20.0,
            true,
            true,
            true,
            Some(StepContact {
                newpos: [1.0, 1.0, 3.0],
                material: 3,
                owner_arm: false,
            }),
        );
        assert_eq!(jump_hard.landing_damage, Some(80.0));
        let light = movement_apply_step(
            HUMAN_ACTION_FALL,
            [1.0, 5.0, 3.0],
            [0.0, -1.0, 0.0],
            4.0,
            false,
            true,
            true,
            Some(StepContact {
                newpos: [1.0, 4.0, 3.0],
                material: 3,
                owner_arm: false,
            }),
        );
        assert_eq!(light.action_state, HUMAN_ACTION_STAND);
        assert_eq!(light.landing_damage, None);
        assert!(!light.selects_doskok);
        assert!(light.wants_person_anim);
        // Hard landing below the damage bar (fall 6, non-jump: 5.5 < 6 but
        // 8 < 6 false): no Hit damage, DOSKOK selected instead.
        let dosk = movement_apply_step(
            HUMAN_ACTION_FALL,
            [1.0, 5.0, 3.0],
            [0.0, -1.0, 0.0],
            6.0,
            false,
            true,
            true,
            Some(StepContact {
                newpos: [1.0, 4.0, 3.0],
                material: 3,
                owner_arm: false,
            }),
        );
        assert_eq!(dosk.action_state, HUMAN_ACTION_STAND);
        assert!(dosk.zeroed_velocity);
        assert_eq!(dosk.landing_damage, None);
        assert!(dosk.selects_doskok);
        assert!(dosk.wants_person_anim);
        // Dead hard landing still stands and zeroes, but requests no anim.
        let dead = movement_apply_step(
            HUMAN_ACTION_FALL,
            [1.0, 5.0, 3.0],
            [0.0, -4.0, 0.0],
            10.0,
            false,
            false,
            true,
            Some(StepContact {
                newpos: [1.0, 1.0, 3.0],
                material: 3,
                owner_arm: false,
            }),
        );
        assert!(!dead.wants_person_anim);
        assert!(dead.zeroed_velocity);
        // FALL with no ground and a travelled step lands by the 0.2 ratio
        // (:5176-5182: 0.05 travelled of 1.0 move < 0.2).
        let ratio_land = movement_apply_step(
            HUMAN_ACTION_FALL,
            [0.0, 5.0, 0.0],
            [1.0, 0.0, 0.0],
            4.0,
            false,
            true,
            false,
            Some(StepContact {
                newpos: [0.05, 5.0, 0.0],
                material: 3,
                owner_arm: false,
            }),
        );
        assert!(ratio_land.landed);
        assert_eq!(ratio_land.action_state, HUMAN_ACTION_STAND);
    }

    #[test]
    fn ratio_sampling_and_fall_entry_follow_body() {
        // Hand-derived from C_human.cpp:4778-4788 (100 ms player period:
        // travelled 0.1 of expected 0.2 with prior ratio 1.0 -> 0.75, clock
        // reset; sub-period calls only accumulate) and :5258-5265 (may_fall
        // with fall > 2 outside FALL enters FALL; JUMP below 10 keeps
        // falling; the 2.0 bound itself does not enter).
        let sampled = movement_update_ratio(
            MoveRatioState {
                move_ratio: 1.0,
                move_time_ms: 0,
                move_pos: [0.0, 0.0, 0.0],
            },
            [0.1, 0.0, 0.0],
            2.0,
            100,
            true,
        );
        assert_eq!(sampled.move_ratio, 0.75);
        assert_eq!(sampled.move_time_ms, 0);
        assert_eq!(sampled.move_pos, [0.1, 0.0, 0.0]);
        let accumulated = movement_update_ratio(
            MoveRatioState {
                move_ratio: 1.0,
                move_time_ms: 0,
                move_pos: [0.0, 0.0, 0.0],
            },
            [0.1, 0.0, 0.0],
            2.0,
            50,
            true,
        );
        assert_eq!(accumulated.move_time_ms, 50);
        assert_eq!(accumulated.move_ratio, 1.0);
        // Zero expected speed maps to 1.0 (the fabs < MRG_ZERO arm, :4782).
        let still = movement_update_ratio(
            MoveRatioState {
                move_ratio: 0.25,
                move_time_ms: 400,
                move_pos: [0.0, 0.0, 0.0],
            },
            [0.0, 0.0, 0.0],
            0.0,
            100,
            false,
        );
        assert_eq!(still.move_ratio, 1.0);
        assert_eq!(still.move_time_ms, 0);
        let entry = movement_fall_entry(HUMAN_ACTION_STAND, 3.0, true);
        assert_eq!(
            entry,
            FallEntry {
                action_state: HUMAN_ACTION_FALL,
                jump_fall: false,
                wants_person_anim: true,
            }
        );
        let jump_entry = movement_fall_entry(HUMAN_ACTION_JUMP, 12.0, true);
        assert_eq!(jump_entry.action_state, HUMAN_ACTION_FALL);
        assert!(jump_entry.jump_fall);
        let jump_hold = movement_fall_entry(HUMAN_ACTION_JUMP, 5.0, true);
        assert_eq!(jump_hold.action_state, HUMAN_ACTION_JUMP);
        assert!(!jump_hold.wants_person_anim);
        let boundary = movement_fall_entry(HUMAN_ACTION_STAND, 2.0, true);
        assert_eq!(boundary.action_state, HUMAN_ACTION_STAND);
        assert!(!boundary.wants_person_anim);
    }

    #[test]
    fn next_coll_owner_merge_reverses_colls_then_ghosts() {
        // Hand-derived from C_human::DynSetNextCollOwnerMult (mafia-re 113b5b6
        // game/actors/C_human.cpp:5308-5322): the ghost-free arm passes colls
        // straight through (None staging), otherwise colls reversed then
        // ghost_colls reversed; is_player never enters the merge (it selects
        // flags2 in test_move_coll_prefix, :5293-5295).
        assert_eq!(collect_next_coll_owners::<u32>(&[1, 2], &[]), None);
        assert!(!next_coll_owners_use_ghost(0));
        assert_eq!(
            collect_next_coll_owners(&[1, 2, 3], &[7, 9]),
            Some(vec![3, 2, 1, 9, 7])
        );
        assert!(next_coll_owners_use_ghost(2));
        // Ghost-only merge still reverses the ghost staging.
        assert_eq!(
            collect_next_coll_owners::<u32>(&[], &[7, 9]),
            Some(vec![9, 7])
        );
    }
}
