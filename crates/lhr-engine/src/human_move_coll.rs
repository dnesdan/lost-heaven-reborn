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
//! DEFERRED with explicit markers, no new owner here: the TestPanak2 capsule
//! sweep itself (no capsule owner exists), the frame anchoring
//! (GetWorldPos/GetWorldDir; no frame owner on this path), the
//! DynSetNextCollOwnerMult plumbing (no colls/ghost_colls owner), the
//! act_mission/game player comparison behind is_player (no mission owner),
//! and the move_coll writeback (C_human 0x414 word; no owner yet).
#![forbid(unsafe_code)]

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
        assert_eq!(
            test_move_coll_torso_lifts(true, false),
            Some([0.55, 1.0])
        );
        assert_eq!(
            test_move_coll_torso_lifts(true, true),
            Some([0.3, 0.6])
        );
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
}
