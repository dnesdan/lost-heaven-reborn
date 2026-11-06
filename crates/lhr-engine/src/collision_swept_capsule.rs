//! First-iteration static core of g_collision::TestPanak2 (GAME 0x5d59c0, RVA 0x1d59c0).
//! Game.exe SHA-256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27,
//! base 0x400000 (RVA = VA - 0x400000). Reviewed upstream source (never vendored):
//! mafia-re 113b5b6 game/collision/g_colls.cpp:60 (TestPanak2 STUB), game/actors/C_human.h:440-454
//! (tgcPanak2 0x8c layout: radii[3] at 0x0, legs at 0xc, body at 0x18, head at 0x24, dir at 0x30,
//! flags2 at 0x3c, flags at 0x40, pos_out at 0x44, normal at 0x50) and game/actors/C_human.cpp:5274
//! (C_human::TestMoveColl descent). REA bodies probe-observed, NOT re-observed in this session:
//! TestPanak2 0x5d59c0-0x5d641d (2654B/603insns, ev_2312cee3) and TestPanak2InVehicle 0x5d6430-0x5d6e0e
//! (ev_20aee0d8). No REA session was opened here.
//! This owner runs exactly one static pass: each of the three capsule spheres sweeps the shared
//! candidate list through the existing sphere_in_tree_level worker, and the nearest hit wins with
//! its pos_out/normal outputs. It wires the owned human_move_coll::test_move_coll_prefix leaves
//! (radii, biased sweep direction, flag words) to the existing sphere stack.
//! DEFERRED with explicit markers, no new owner here: the slide/iterate loop around this single
//! pass, the DAT_00648240 global (last-distance store; misses here leave no global behind), the
//! 0x5ca370 clamp on the swept result, dynamic dispatch (selector != -1 with dynamic candidates),
//! flag-filtering beyond the SphereParam reject-mask pass-through, TestPanak2InVehicle (GAME
//! 0x5d6430), frame anchoring of legs/body/head (GetWorldPos/GetWorldDir; the caller supplies
//! already-anchored centres), DynSetNextCollOwnerMult plumbing (no colls/ghost_colls owner), and
//! the C_human move_coll (0x414) writeback with its hit/miss result selection.
#![forbid(unsafe_code)]

use crate::{
    collision_dispatch::CollisionPrimitive,
    collision_swept::{normalized, SphereParam, SphereSweepError, SphereSweepRequest},
    collision_swept_world::sphere_in_tree_level,
    human_move_coll::TestMoveCollProbe,
};

/// Static projection of tgcPanak2 bytes 0x0-0x40: the three sphere centres with their radii share
/// one sweep displacement; selector/reject_mask ride the existing SphereSweepRequest contract.
/// flags2-to-selector mapping stays DEFERRED with dynamic dispatch; see from_prefix. NOTE: a
/// non-(-1) selector currently has no filtering effect in this pass (the dynamic gate lives in
/// test_sphere_b, which this candidates-slice path bypasses); it is carried for the deferred
/// dynamic dispatch only.
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

/// Exactly one pass of 3x sphere_in_tree_level over the same candidates, nearest win. Strictly
/// smaller best_distance_squared replaces the leader, so equal-distance hits retain legs, then
/// body, then head. A full miss is Ok(None); host errors propagate without partial output.
pub fn panak2_static_once(
    candidates: &[CollisionPrimitive],
    req: &Panak2Request,
) -> Result<Option<Panak2StaticHit>, SphereSweepError> {
    let origins = [req.legs, req.body, req.head];
    let mut best: Option<(usize, SphereParam)> = None;
    for (index, (origin, radius)) in origins.into_iter().zip(req.radii).enumerate() {
        let mut query = SphereParam::new(&SphereSweepRequest {
            origin,
            delta: req.delta,
            radius,
            selector: req.selector,
            reject_mask: req.reject_mask,
        })?;
        if !sphere_in_tree_level(candidates, &mut query)? {
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
}
