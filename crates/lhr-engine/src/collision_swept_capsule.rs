//! First-iteration static core of g_collision::TestPanak2 (GAME 0x5d59c0, RVA 0x1d59c0).
//! Game.exe SHA-256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27,
//! base 0x400000 (RVA = VA - 0x400000). Reviewed upstream source (never vendored):
//! mafia-re 113b5b6 game/collision/g_colls.cpp:60 (TestPanak2 STUB), game/actors/C_human.h:440-454
//! (tgcPanak2 0x8c layout: radii[3] at 0x0, legs at 0xc, body at 0x18, head at 0x24, dir at 0x30,
//! flags2 at 0x3c, flags at 0x40, pos_out at 0x44, normal at 0x50) and game/actors/C_human.cpp:5274
//! (C_human::TestMoveColl descent). REA bodies probe-observed, NOT re-observed in this session:
//! TestPanak2 0x5d59c0-0x5d641d (2654B/603insns, ev_2312cee3) and TestPanak2InVehicle 0x5d6430-0x5d6e0e
//! (ev_20aee0d8). No REA session was opened here.
//! This owner runs one static pass per slide step: each of the three capsule
//! spheres sweeps the shared candidate list through the existing sphere_in_tree_level
//! worker, and the nearest hit wins with its pos_out/normal outputs. It wires the owned
//! human_move_coll::test_move_coll_prefix leaves (radii, biased sweep direction, flag
//! words) to the existing sphere stack, then iterates the pass with wall sliding until
//! the remainder is consumed, misses, or the caller-supplied slide bound is reached
//! (GAME 0x5d59c0, RVA 0x1d59c0, binary range 0x5d59c0-0x5d641d REA-observed to
//! slide/repeat; the exact original pass bound is unproven, so the bound stays a caller
//! argument with a working-model default for the TestMoveColl glue).
//! Dynamic dispatch (selector != -1 with caller-supplied dynamic candidates swept after
//! the static list, nearest win) and the reject-mask pass-through on both lists are owned
//! here; the static-only gate (selector == -1 ignores the dynamic list) mirrors the
//! test_sphere_b/test_line_v `selector != -1 && dynamic present` dispatch condition.
//! TestPanak2InVehicle (GAME 0x5d6430, RVA 0x1d6430) shares this core over the vehicle's
//! dyn_colls slice (single pass; whether the original iterates inside InVehicle is
//! unproven, so iteration there stays DEFERRED).
//! DEFERRED with explicit markers, no new owner here: the flags2-to-selector mapping
//! table (no evidenced table in the vendored stub; the caller supplies the selector and
//! flags ride the reject mask), the DAT_00648240 global itself (threaded as the explicit
//! last-distance out value, store-on-miss semantics unproven), the 0x5ca370 clamp
//! (site and bound unproven; no clamp is applied), frame anchoring of legs/body/head
//! (GetWorldPos/GetWorldDir; the caller supplies already-anchored centres),
//! DynSetNextCollOwnerMult plumbing (no colls/ghost_colls owner), and the C_human
//! move_coll (0x414) writeback with its hit/miss result selection.
#![forbid(unsafe_code)]

use crate::{
    collision_dispatch::CollisionPrimitive,
    collision_swept::{normalized, SphereParam, SphereSweepError, SphereSweepRequest},
    collision_swept_world::sphere_in_tree_level,
    human_move_coll::{
        test_move_coll_hit_result, test_move_coll_miss_result, test_move_coll_prefix,
        TestMoveCollProbe,
    },
};

/// Static projection of tgcPanak2 bytes 0x0-0x40: the three sphere centres with their radii share
/// one sweep displacement; selector/reject_mask ride the existing SphereSweepRequest contract.
/// flags2-to-selector mapping stays DEFERRED with dynamic dispatch; see from_prefix. A
/// non-(-1) selector additionally sweeps the caller-supplied dynamic list in
/// panak2_static_once_full (the static-only panak2_static_once path ignores it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Panak2Request {
    pub legs: [f32; 3],
    pub body: [f32; 3],
    pub head: [f32; 3],
    pub delta: [f32; 3],
    pub radii: [f32; 3],
    pub selector: i32,
    pub reject_mask: u32,
}

/// Winning sphere of the single static pass: index 0 legs, 1 body, 2 head. pos_out is the stopped
/// centre of the winning sphere and normal its outward surface normal, both following the
/// test_sphere_b_continue output conversion (negated stored normal). ASSUMPTION (unverified
/// against the REA body, kept explicit): upstream evidence says pos_out is "where the legs'
/// sphere stops" (C_human.h:451) and TestMoveColl subtracts the legs lift unconditionally, so a
/// body/head win may attribute pos_out to the legs frame instead; per-sphere output attribution
/// stays DEFERRED pending REA verification.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Panak2StaticHit {
    pub sphere: usize,
    pub primitive: u32,
    pub distance_squared: f32,
    pub pos_out: [f32; 3],
    pub normal: [f32; 3],
}

impl Panak2Request {
    /// Wire the owned TestMoveColl prefix to this static core: radii, biased sweep direction and
    /// flag words come from the probe, already-anchored centres and the dynamic selector from the
    /// caller (frame anchoring and flags2-to-selector mapping stay DEFERRED).
    pub fn from_prefix(
        legs: [f32; 3],
        body: [f32; 3],
        head: [f32; 3],
        prefix: &TestMoveCollProbe,
        selector: i32,
    ) -> Self {
        Self {
            legs,
            body,
            head,
            delta: prefix.biased_dir,
            radii: prefix.radii,
            selector,
            reject_mask: prefix.flags,
        }
    }
}

/// GAME 0x5d59c0 (RVA 0x1d59c0) g_collision::TestPanak2 entry (binary range
/// 0x5d59c0-0x5d641d REA-observed to slide/repeat; no REA session opened here).
pub const PANAK2_RVA_FIRST: u32 = 0x1d59c0;
/// GAME 0x5d6430 (RVA 0x1d6430) g_collision::TestPanak2InVehicle entry.
pub const PANAK2_IN_VEHICLE_RVA: u32 = 0x1d6430;
/// DAT_00648240 (RVA 0x248240): last-distance store of the TestPanak2 body.
/// The global itself has no owner; the slide outcome threads its value as an
/// explicit field instead. Store-on-miss semantics are unproven (DEFERRED): a
/// clean first-pass miss yields None (global assumed untouched).
pub const PANAK2_LAST_DISTANCE_RVA: u32 = 0x248240;
/// 0x5ca370 clamp site named by the task brief. Site and bound are unproven
/// (DEFERRED): no clamp is applied anywhere in this core.
pub const PANAK2_CLAMP_SITE_VA: u32 = 0x5ca370;
/// flags2-to-selector mapping table: DEFERRED (no evidenced table; the
/// mafia-re 113b5b6 g_colls.cpp:60-71 TestPanak2/TestPanak2InVehicle bodies are
/// stubs). The caller supplies Panak2Request.selector directly; TestMoveColl
/// callers pass -1 (static only) and TestMoveCollInOwner callers pass -1 (the
/// InOwner body sets only flags, no flags2 per C_human.cpp:5341).
pub const PANAK2_FLAGS2_SELECTOR_DEFERRED: bool = true;
/// GAME 0x57cfe0 (RVA 0x17cfe0) `C_human::DynSetNextCollOwnerMult` contract
/// around the `TestMoveColl` sweep (mafia-re 113b5b6
/// game/actors/C_human.cpp:5296,5308-5322): the staged merge itself
/// (`colls` reversed then staged `ghost_colls` reversed, ghost-free arm passes
/// `colls` straight through) is owned by `human_actor::HumanState` and
/// `human_move_coll::collect_next_coll_owners`; the `is_player` distinction
/// selects `flags2` in `human_move_coll::test_move_coll_prefix` (:5293-5295).
/// This core consumes only the already-merged candidate slices, so the
/// `g_collision::DynSetNextCollOwnerMult` call and the `next_coll_owners`
/// global stay with the collision manager (DEFERRED). Pedestrian `C_entity`
/// AI Tick (GAME 0x508150) refreshing the source `ai_ghost` vector stays
/// DEFERRED as well.
pub const PANAK2_GHOST_MERGE_RVA: u32 = 0x17cfe0;
/// Working-model slide bound for the TestMoveColl glue (descriptive, NOT
/// original: the binary range proves iteration, not the count). The core takes
/// the bound as a caller argument; the glue passes this default.
pub const PANAK2_WORKING_MAX_SLIDES: usize = 3;
/// Working-model rest threshold: the slide loop stops when the squared length
/// of the slid remainder drops to this (descriptive, NOT original).
pub const PANAK2_SLIDE_REST_SQUARED: f32 = 1e-12;

/// Static gate shared with test_sphere_b/test_line_v: selector == -1 sweeps the
/// static list only; selector != -1 additionally sweeps the caller-supplied
/// dynamic list with the same reject mask, nearest win across both lists.
pub fn panak2_uses_dynamic(selector: i32) -> bool {
    selector != -1
}

/// Staged owner count for the `TestMoveColl` sweep contract (mafia-re 113b5b6
/// game/actors/C_human.cpp:5310-5322): `colls.len() + ghost_len` once ghosts
/// are staged, else `colls.len()` (the ghost-free arm passes `colls` straight
/// through). Marker-only length leaf for the `DynSetNextCollOwnerMult` merge;
/// the exclusion itself stays with the collision manager (DEFERRED).
pub fn panak2_next_owner_count(colls_len: usize, ghost_len: usize) -> usize {
    if ghost_len == 0 {
        colls_len
    } else {
        colls_len.saturating_add(ghost_len)
    }
}

fn panak2_dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn panak2_sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn panak2_add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn panak2_scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

/// One static pass over the static list plus, when `panak2_uses_dynamic`
/// (selector != -1), the caller-supplied dynamic list. The same reject mask
/// filters both lists (pass-through beyond the mask stays DEFERRED, matching
/// the SphereParam contract). Static candidates sweep first, so equal-distance
/// hits retain legs, then body, then head, then static before dynamic.
/// A full miss is Ok(None); host errors propagate without partial output.
pub fn panak2_static_once_full(
    static_candidates: &[CollisionPrimitive],
    dynamic_candidates: &[CollisionPrimitive],
    req: &Panak2Request,
) -> Result<Option<Panak2StaticHit>, SphereSweepError> {
    let origins = [req.legs, req.body, req.head];
    let mut best: Option<(usize, SphereParam)> = None;
    let lists: [&[CollisionPrimitive]; 2] = if panak2_uses_dynamic(req.selector) {
        [static_candidates, dynamic_candidates]
    } else {
        [static_candidates, &[]]
    };
    for (index, (origin, radius)) in origins.into_iter().zip(req.radii).enumerate() {
        let mut query = SphereParam::new(&SphereSweepRequest {
            origin,
            delta: req.delta,
            radius,
            selector: req.selector,
            reject_mask: req.reject_mask,
        })?;
        let mut pass_hit = false;
        for candidates in lists {
            pass_hit |= sphere_in_tree_level(candidates, &mut query)?;
        }
        if !pass_hit {
            continue;
        }
        let replace = match &best {
            None => true,
            Some((_, leader)) => query.best_distance_squared < leader.best_distance_squared,
        };
        if replace {
            best = Some((index, query));
        }
    }
    let Some((sphere, query)) = best else {
        return Ok(None);
    };
    let normal = normalized(query.normal, [0, 1, 2])?;
    Ok(Some(Panak2StaticHit {
        sphere,
        primitive: query.primitive.expect("native hit carries a primitive"),
        distance_squared: query.best_distance_squared,
        pos_out: query.point,
        normal: normal.map(|x| -x),
    }))
}

/// Outcome of the TestPanak2 slide loop (GAME 0x5d59c0, RVA 0x1d59c0).
/// `stop` is the legs-frame rest position, `last_distance_squared` the explicit
/// threading of the DAT_00648240 (RVA 0x248240) last-distance store (None on a
/// clean first-pass miss). `slides` counts repeat passes after the first.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Panak2SlideOutcome {
    pub hit: bool,
    pub stop: [f32; 3],
    pub normal: Option<[f32; 3]>,
    pub slides: u32,
    pub last_distance_squared: Option<f32>,
}

/// Slide/iterate loop around [`panak2_static_once_full`]: on a hit the capsule
/// rigidly advances by the consumed vector (pos_out minus the winning sphere's
/// origin), the remainder (delta minus consumed) is projected onto the hit
/// plane, and the pass repeats until a miss, the remainder rests at or below
/// [`PANAK2_SLIDE_REST_SQUARED`], or `max_slides` repeat passes run. On a miss
/// the remainder is consumed into `stop`. The exact original pass bound and the
/// 0x5ca370 clamp application are unproven (DEFERRED): exhaustion drops the
/// leftover remainder. Per-sphere pos_out attribution beyond the legs frame
/// stays DEFERRED as in the single-pass core.
pub fn panak2_slide(
    static_candidates: &[CollisionPrimitive],
    dynamic_candidates: &[CollisionPrimitive],
    req: &Panak2Request,
    max_slides: usize,
) -> Result<Panak2SlideOutcome, SphereSweepError> {
    let mut origins = [req.legs, req.body, req.head];
    let mut delta = req.delta;
    let mut pass_req = *req;
    let mut slides = 0u32;
    let mut last_hit: Option<Panak2StaticHit> = None;
    loop {
        pass_req.legs = origins[0];
        pass_req.body = origins[1];
        pass_req.head = origins[2];
        pass_req.delta = delta;
        let hit = panak2_static_once_full(static_candidates, dynamic_candidates, &pass_req)?;
        let Some(hit) = hit else {
            let stop = panak2_add(origins[0], delta);
            let last_distance_squared = last_hit.map(|previous| previous.distance_squared);
            return Ok(Panak2SlideOutcome {
                hit: last_hit.is_some(),
                stop,
                normal: last_hit.map(|previous| previous.normal),
                slides,
                last_distance_squared,
            });
        };
        last_hit = Some(hit);
        let consumed = panak2_sub(hit.pos_out, origins[hit.sphere]);
        for origin in &mut origins {
            *origin = panak2_add(*origin, consumed);
        }
        let mut remainder = panak2_sub(delta, consumed);
        let along = panak2_dot(remainder, hit.normal);
        remainder = panak2_sub(remainder, panak2_scale(hit.normal, along));
        delta = remainder;
        if panak2_dot(remainder, remainder) <= PANAK2_SLIDE_REST_SQUARED {
            return Ok(Panak2SlideOutcome {
                hit: true,
                stop: origins[0],
                normal: Some(hit.normal),
                slides,
                last_distance_squared: Some(hit.distance_squared),
            });
        }
        slides += 1;
        if slides as usize > max_slides {
            return Ok(Panak2SlideOutcome {
                hit: true,
                stop: origins[0],
                normal: Some(hit.normal),
                slides,
                last_distance_squared: Some(hit.distance_squared),
            });
        }
    }
}

/// GAME 0x5d6430 (RVA 0x1d6430) TestPanak2InVehicle single pass over the
/// vehicle's dyn_colls slice (mafia-re 113b5b6 game/actors/C_human.cpp:5343-5350:
/// the railway's `dyn_colls[wagon]` or the car_ex's `dyn_colls` for a human with
/// no seat). Whether the original iterates inside InVehicle is unproven
/// (DEFERRED); callers needing repeats use [`panak2_slide`] over the same slice.
pub fn panak2_in_vehicle_once(
    dyn_colls: &[CollisionPrimitive],
    req: &Panak2Request,
) -> Result<Option<Panak2StaticHit>, SphereSweepError> {
    panak2_static_once_full(dyn_colls, &[], req)
}

/// Landed single-pass core kept for the existing glue: exactly one pass of 3x
/// sphere_in_tree_level over the same candidates, nearest win. Strictly smaller
/// best_distance_squared replaces the leader, so equal-distance hits retain legs,
/// then body, then head. A full miss is Ok(None); host errors propagate without
/// partial output. The dynamic list is empty here (selector == -1 static gate);
/// dynamic callers use [`panak2_static_once_full`].
pub fn panak2_static_once(
    candidates: &[CollisionPrimitive],
    req: &Panak2Request,
) -> Result<Option<Panak2StaticHit>, SphereSweepError> {
    panak2_static_once_full(candidates, &[], req)
}

/// Pure glue outcome of one C_human::TestMoveColl static pass: the selected
/// result vector plus the winning static hit, if any. `result` is the only
/// value the original returns by value; the deferred move_coll (0x414)
/// writeback would consume `hit` elsewhere.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TestMoveCollStaticOutcome {
    pub result: [f32; 3],
    pub hit: Option<Panak2StaticHit>,
}

/// Glue between the two landed units, no new RVA (recorded in comments
/// only): GAME 0x57cb60 (RVA 0x17cb60) C_human::TestMoveColl with GAME
/// 0x5d59c0 (RVA 0x1d59c0) g_collision::TestPanak2. Game.exe SHA-256
/// 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27,
/// base 0x400000. Reviewed upstream assembly order, never vendored:
/// mafia-re 113b5b6 game/actors/C_human.cpp:5274-5308 (prefix leaves ->
/// DynSetNextCollOwnerMult -> TestPanak2 -> miss from+dir / hit pos_out-0.4).
/// Runs human_move_coll::test_move_coll_prefix, then the single static pass
/// panak2_static_once over the caller-supplied candidates, then the owned
/// result selection: a miss returns test_move_coll_miss_result(from, dir)
/// with the original UNBIASED displacement (the biased sweep direction stays
/// inside the request only), a hit returns test_move_coll_hit_result of the
/// winning pos_out (pos_out.y - 0.4). Centres are caller-supplied already-
/// anchored values, not read here.
/// DEFERRED with explicit markers, no new owner here: frame anchoring of
/// legs/body/head (GetWorldPos/GetWorldDir), DynSetNextCollOwnerMult and
/// colls/ghost_colls plumbing around the sweep, is_player resolution against
/// act_mission/game player (the caller supplies the bool), the
/// flags2-to-selector table (the caller supplies the static selector), the
/// DAT_00648240 global itself (threaded as the outcome's last-distance value),
/// the 0x5ca370 clamp (no clamp applied), and the C_human move_coll (0x414)
/// writeback.
pub fn test_move_coll_static_result(
    candidates: &[CollisionPrimitive],
    alive: bool,
    crouching: bool,
    is_player: bool,
    from: [f32; 3],
    dir: [f32; 3],
    legs: [f32; 3],
    body: [f32; 3],
    head: [f32; 3],
    selector: i32,
) -> Result<TestMoveCollStaticOutcome, SphereSweepError> {
    let prefix = test_move_coll_prefix(alive, crouching, is_player, from, dir);
    let req = Panak2Request::from_prefix(legs, body, head, &prefix, selector);
    match panak2_static_once(candidates, &req)? {
        Some(hit) => Ok(TestMoveCollStaticOutcome {
            result: test_move_coll_hit_result(hit.pos_out),
            hit: Some(hit),
        }),
        None => Ok(TestMoveCollStaticOutcome {
            result: test_move_coll_miss_result(from, dir),
            hit: None,
        }),
    }
}

/// Full TestMoveColl sweep (GAME 0x57cb60 with GAME 0x5d59c0): the owned prefix
/// leaves plus the [`panak2_slide`] loop over the static and caller-supplied
/// dynamic candidate lists, then the owned result selection: a miss returns
/// test_move_coll_miss_result(from, dir) with the original UNBIASED displacement,
/// a hit returns test_move_coll_hit_result of the slid legs-frame stop
/// (stop.y - 0.4). `max_slides` bounds the repeat passes; the TestMoveColl callers
/// pass [`PANAK2_WORKING_MAX_SLIDES`] (working model, exact original bound
/// DEFERRED). `hit` stays None: the slide has no single static hit, the slid
/// stop carries the rest position. The DAT_00648240 last distance rides the
/// outcome; the move_coll (0x414) writeback stays DEFERRED with the C_human
/// owner.
pub fn test_move_coll_slide_result(
    static_candidates: &[CollisionPrimitive],
    dynamic_candidates: &[CollisionPrimitive],
    alive: bool,
    crouching: bool,
    is_player: bool,
    from: [f32; 3],
    dir: [f32; 3],
    legs: [f32; 3],
    body: [f32; 3],
    head: [f32; 3],
    selector: i32,
    max_slides: usize,
) -> Result<TestMoveCollStaticOutcome, SphereSweepError> {
    let prefix = test_move_coll_prefix(alive, crouching, is_player, from, dir);
    let req = Panak2Request::from_prefix(legs, body, head, &prefix, selector);
    let slide = panak2_slide(static_candidates, dynamic_candidates, &req, max_slides)?;
    if slide.hit {
        Ok(TestMoveCollStaticOutcome {
            result: test_move_coll_hit_result(slide.stop),
            hit: None,
        })
    } else {
        Ok(TestMoveCollStaticOutcome {
            result: test_move_coll_miss_result(from, dir),
            hit: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision_line::Triangle;

    fn floor(identity: u32) -> CollisionPrimitive {
        CollisionPrimitive::Triangle(Triangle {
            properties: 3,
            vertices: [[-2., 0., -2.], [-2., 0., 2.], [2., 0., -2.]],
            plane: [0., -1., 0., 0.],
            identity,
        })
    }

    /// Mask-clean floor for the glued result mapping: the real prefix flags
    /// (0x80002, effective 0x80022 after the worker's | 0x20 augmentation in
    /// SphereParam::new) reject any primitive sharing a mask bit under the
    /// owned reject-on-overlap rule, so the proven properties=3 fixture would
    /// miss here by mask, not by geometry. Flag-filtering semantics beyond
    /// the mask stay DEFERRED; this fixture pins only the miss/hit mapping.
    fn floor_unflagged(identity: u32) -> CollisionPrimitive {
        CollisionPrimitive::Triangle(Triangle {
            properties: 0,
            vertices: [[-2., 0., -2.], [-2., 0., 2.], [2., 0., -2.]],
            plane: [0., -1., 0., 0.],
            identity,
        })
    }

    fn request() -> Panak2Request {
        Panak2Request {
            legs: [-0.5, 2., -0.5],
            body: [-0.5, 2.5, -0.5],
            head: [-0.5, 3., -0.5],
            delta: [0., -4., 0.],
            radii: [0.5, 0.5, 0.5],
            selector: -1,
            reject_mask: 0,
        }
    }

    #[test]
    fn all_hit_nearest_win_is_legs_with_record_outputs() {
        // Hand-derived from the proven floor fixture (collision_swept_world static_grid test):
        // legs contacts at 1.5 (2.25), body at 2.0 (4.0), head at 2.5 (6.25); all values are
        // exact binary fractions, so the winner and its outputs are bit-exact.
        let candidates = [floor(7)];
        let hit = panak2_static_once(&candidates, &request())
            .unwrap()
            .unwrap();
        assert_eq!(hit.sphere, 0);
        assert_eq!(hit.primitive, 7);
        assert_eq!(hit.distance_squared, 2.25);
        assert_eq!(hit.pos_out, [-0.5, 0.5, -0.5]);
        assert_eq!(hit.normal, [-0., 1., -0.]);
    }

    #[test]
    fn upward_sweep_and_empty_candidates_miss() {
        let candidates = [floor(7)];
        let mut req = request();
        req.delta = [0., 4., 0.];
        assert_eq!(panak2_static_once(&candidates, &req), Ok(None));
        assert_eq!(panak2_static_once(&[], &request()), Ok(None));
    }

    #[test]
    fn parked_legs_yields_body_win_with_own_radius() {
        // Hand-derived: legs at y 10 needs 9.5 of travel against a 4.0 sweep, so its face hit is
        // rejected past best_distance_squared (|delta|^2); body (contact 2.0, 4.0) beats head
        // (contact 2.5, 6.25), all exact binary fractions.
        let candidates = [floor(7)];
        let mut req = request();
        req.legs = [-0.5, 10., -0.5];
        let hit = panak2_static_once(&candidates, &req).unwrap().unwrap();
        assert_eq!(hit.sphere, 1);
        assert_eq!(hit.primitive, 7);
        assert_eq!(hit.distance_squared, 4.);
        assert_eq!(hit.pos_out, [-0.5, 0.5, -0.5]);
        assert_eq!(hit.normal, [-0., 1., -0.]);
    }

    #[test]
    fn from_prefix_carries_radii_biased_dir_and_flags() {
        // Hand-derived wiring check against the owned prefix leaves (human_move_coll):
        // radii [0.4, 0.25, 0.35], y-only bias, alive flags 0x80002.
        let probe = crate::human_move_coll::test_move_coll_prefix(
            true,
            false,
            false,
            [1., 2., 3.],
            [0.5, -0.25, -1.],
        );
        let req = Panak2Request::from_prefix(
            [-0.5, 2., -0.5],
            [-0.5, 2.5, -0.5],
            [-0.5, 3., -0.5],
            &probe,
            -1,
        );
        assert_eq!(req.radii, crate::human_move_coll::TEST_MOVE_COLL_RADII);
        assert_eq!(req.delta, [0.5, -0.265, -1.]);
        assert_eq!(req.reject_mask, 0x80002);
        assert_eq!(req.selector, -1);
    }

    #[test]
    fn static_result_miss_keeps_unbiased_dir() {
        // Hand-derived from C_human.cpp:5300-5301 with the mask-clean floor
        // (upward geometry misses; the proven properties=3 floor would also
        // miss here by mask, so the unflagged fixture isolates geometry):
        // the sweep runs the biased direction ([0, 3.985, 0] from
        // dir [0, 4, 0]) upward into a miss, while the returned result adds
        // the original unbiased displacement. All miss arithmetic below is
        // exact binary fractions.
        let candidates = [floor_unflagged(7)];
        let from = [1., 2., 3.];
        let dir = [0., 4., 0.];
        let outcome = test_move_coll_static_result(
            &candidates,
            true,
            false,
            false,
            from,
            dir,
            [-0.5, 2., -0.5],
            [-0.5, 2.5, -0.5],
            [-0.5, 3., -0.5],
            -1,
        )
        .unwrap();
        assert_eq!(outcome.hit, None);
        assert_eq!(outcome.result, [1., 6., 3.]);
        assert_eq!(
            outcome.result,
            crate::human_move_coll::test_move_coll_miss_result(from, dir)
        );
    }

    #[test]
    fn static_result_hit_subtracts_legs_radius() {
        // Hand-derived from C_human.cpp:5303 with the mask-clean floor
        // fixture (the proven properties=3 floor would miss here by mask,
        // not geometry; see floor_unflagged): dir [0, -4, 0] sweeps biased
        // [0, -4.015, 0] down through the floor, so legs (contact 1.6) beats
        // body (2.25) and head (2.65) and the result drops the winning
        // pos_out by the legs radius on y only. The exact stopped height is
        // not hand-pinned (0.4 is not a binary fraction); the wiring
        // assertions below check the owned 0.4 leaf by construction instead.
        let candidates = [floor_unflagged(7)];
        let from = [1., 2., 3.];
        let dir = [0., -4., 0.];
        let outcome = test_move_coll_static_result(
            &candidates,
            true,
            false,
            false,
            from,
            dir,
            [-0.5, 2., -0.5],
            [-0.5, 2.5, -0.5],
            [-0.5, 3., -0.5],
            -1,
        )
        .unwrap();
        let hit = outcome.hit.expect("downward sweep over the floor hits");
        assert_eq!(hit.sphere, 0);
        assert_eq!(
            outcome.result,
            crate::human_move_coll::test_move_coll_hit_result(hit.pos_out)
        );
        assert_eq!(outcome.result[0], hit.pos_out[0]);
        assert_eq!(outcome.result[2], hit.pos_out[2]);
        assert_eq!(
            outcome.result[1],
            hit.pos_out[1] - crate::human_move_coll::TEST_MOVE_COLL_RESULT_DROP
        );
    }

    #[test]
    fn static_result_empty_candidates_misses() {
        // Hand-derived from C_human.cpp:5300-5301: with no candidates the
        // downward sweep that hits the floor above must still miss, returning
        // from + unbiased dir ([1, -2, 3], exact binary fractions).
        let from = [1., 2., 3.];
        let dir = [0., -4., 0.];
        let outcome = test_move_coll_static_result(
            &[],
            true,
            false,
            false,
            from,
            dir,
            [-0.5, 2., -0.5],
            [-0.5, 2.5, -0.5],
            [-0.5, 3., -0.5],
            -1,
        )
        .unwrap();
        assert_eq!(outcome.hit, None);
        assert_eq!(outcome.result, [1., -2., 3.]);
    }

    #[test]
    fn dynamic_list_ignored_for_static_selector_and_swept_otherwise() {
        // Static-only gate mirroring the test_sphere_b/test_line_v
        // `selector != -1 && dynamic present` dispatch condition: with
        // selector -1 the dynamic-only floor is invisible (miss); with
        // selector 0 the same floor hits with the legs sphere winning.
        let dynamic = [floor_unflagged(7)];
        let mut req = request();
        req.delta = [0., -4., 0.];
        assert_eq!(panak2_static_once_full(&[], &dynamic, &req), Ok(None));
        req.selector = 0;
        let hit = panak2_static_once_full(&[], &dynamic, &req)
            .unwrap()
            .expect("dynamic floor under a downward sweep hits");
        assert_eq!(hit.sphere, 0);
        assert_eq!(hit.primitive, 7);
        assert_eq!(hit.normal, [-0., 1., -0.]);
    }

    #[test]
    fn straight_down_slide_rests_at_first_contact() {
        // Angled-free descent over the mask-clean floor: the first pass hits
        // and the slid remainder is parallel to the contact normal, so the
        // loop rests with zero repeat passes at the single-pass contact.
        let candidates = [floor_unflagged(7)];
        let mut req = request();
        req.delta = [0., -4., 0.];
        req.selector = -1;
        let single = panak2_static_once(&candidates, &req).unwrap().unwrap();
        let slide = panak2_slide(&candidates, &[], &req, PANAK2_WORKING_MAX_SLIDES).unwrap();
        assert!(slide.hit);
        assert_eq!(slide.slides, 0);
        assert_eq!(slide.stop, single.pos_out);
        assert_eq!(slide.normal, Some(single.normal));
        assert_eq!(slide.last_distance_squared, Some(single.distance_squared));
    }

    #[test]
    fn angled_descent_slides_tangential_remainder_into_miss() {
        // Descent with a sideways component: the first pass hits the floor,
        // the remainder projected onto the plane runs horizontally at contact
        // height, and the second pass misses, consuming it into the stop.
        let candidates = [floor_unflagged(7)];
        let mut req = request();
        req.delta = [2., -4., 0.];
        let first = panak2_static_once(&candidates, &req).unwrap().unwrap();
        let slide = panak2_slide(&candidates, &[], &req, PANAK2_WORKING_MAX_SLIDES).unwrap();
        assert!(slide.hit);
        assert_eq!(slide.slides, 1);
        assert_eq!(slide.normal, Some(first.normal));
        assert_eq!(slide.last_distance_squared, Some(first.distance_squared));
        assert!(slide.stop[0] > first.pos_out[0]);
        assert_eq!(slide.stop[1], first.pos_out[1]);
        assert_eq!(slide.stop[2], first.pos_out[2]);
    }

    #[test]
    fn slide_respects_zero_bound_and_reports_it() {
        // With max_slides 0 the loop keeps the first contact and drops the
        // slid remainder (exact original bound/exhaustion behavior DEFERRED).
        let candidates = [floor_unflagged(7)];
        let mut req = request();
        req.delta = [2., -4., 0.];
        let first = panak2_static_once(&candidates, &req).unwrap().unwrap();
        let slide = panak2_slide(&candidates, &[], &req, 0).unwrap();
        assert!(slide.hit);
        assert_eq!(slide.slides, 1);
        assert_eq!(slide.stop[1], first.pos_out[1]);
        assert_eq!(slide.last_distance_squared, Some(first.distance_squared));
    }

    #[test]
    fn in_vehicle_single_pass_hits_dyn_colls_and_misses_empty() {
        // GAME 0x5d6430 (RVA 0x1d6430) TestPanak2InVehicle over the vehicle
        // dyn_colls slice (C_human.cpp:5343-5350): downward sweep hits the
        // dyn floor, upward sweep and an empty slice miss.
        let dyn_colls = [floor_unflagged(7)];
        let mut req = request();
        req.delta = [0., -4., 0.];
        let hit = panak2_in_vehicle_once(&dyn_colls, &req)
            .unwrap()
            .expect("in-vehicle downward sweep over the dyn floor hits");
        assert_eq!(hit.sphere, 0);
        assert_eq!(hit.normal, [-0., 1., -0.]);
        req.delta = [0., 4., 0.];
        assert_eq!(panak2_in_vehicle_once(&dyn_colls, &req), Ok(None));
        req.delta = [0., -4., 0.];
        assert_eq!(panak2_in_vehicle_once(&[], &req), Ok(None));
    }

    #[test]
    fn slide_glue_selects_hit_stop_or_unbiased_miss() {
        // Full TestMoveColl sweep (GAME 0x57cb60 with 0x5d59c0): the downward
        // sweep returns the slid stop dropped by the owned 0.4 leaf on y only,
        // the upward sweep returns from + unbiased dir ([1, 6, 3]).
        let candidates = [floor_unflagged(7)];
        let from = [1., 2., 3.];
        let legs = [-0.5, 2., -0.5];
        let body = [-0.5, 2.5, -0.5];
        let head = [-0.5, 3., -0.5];
        let hit_outcome = test_move_coll_slide_result(
            &candidates,
            &[],
            true,
            false,
            false,
            from,
            [0., -4., 0.],
            legs,
            body,
            head,
            -1,
            PANAK2_WORKING_MAX_SLIDES,
        )
        .unwrap();
        let prefix =
            crate::human_move_coll::test_move_coll_prefix(true, false, false, from, [0., -4., 0.]);
        let req = Panak2Request::from_prefix(legs, body, head, &prefix, -1);
        let slide = panak2_slide(&candidates, &[], &req, PANAK2_WORKING_MAX_SLIDES).unwrap();
        assert!(slide.hit);
        assert_eq!(
            hit_outcome.result,
            crate::human_move_coll::test_move_coll_hit_result(slide.stop)
        );
        assert_eq!(hit_outcome.result[0], slide.stop[0]);
        assert_eq!(hit_outcome.result[2], slide.stop[2]);
        assert_eq!(
            hit_outcome.result[1],
            slide.stop[1] - crate::human_move_coll::TEST_MOVE_COLL_RESULT_DROP
        );
        let miss_outcome = test_move_coll_slide_result(
            &candidates,
            &[],
            true,
            false,
            false,
            from,
            [0., 4., 0.],
            legs,
            body,
            head,
            -1,
            PANAK2_WORKING_MAX_SLIDES,
        )
        .unwrap();
        assert_eq!(miss_outcome.result, [1., 6., 3.]);
    }

    #[test]
    fn ghost_merge_owner_count_matches_dyn_set_next_coll_owner_mult_arms() {
        // Hand-derived from C_human::DynSetNextCollOwnerMult (mafia-re 113b5b6
        // game/actors/C_human.cpp:5310-5322): ghost-free passes colls straight
        // through, otherwise colls plus staged ghosts.
        assert_eq!(panak2_next_owner_count(3, 0), 3);
        assert_eq!(panak2_next_owner_count(3, 2), 5);
        assert_eq!(panak2_next_owner_count(0, 2), 2);
        assert_eq!(PANAK2_GHOST_MERGE_RVA, 0x17cfe0);
    }
}
