//! Playable first-mission slice: on-foot walking, cutscene playback and
//! MISE01 objective tracking.
//!
//! These are host-authored gameplay controllers, not native ports: no new
//! entry RVA is claimed and no original-function credit is taken. The step
//! math reuses the evidenced `human_move_coll` leaves (gravity 9.81,
//! ground projection, small-step boost, STAND/FALL resolution); walk/run
//! speeds, the flat-ground assumption and the taxi-stand marker below are
//! explicit host tuning. Native `C_human` movement selection, AI, collision
//! managers, `Intern_UseCar`, animation and mission scripts stay DEFERRED.
//!
//! Reference: mafia-re at pin `fe7e746`
//! (`game/actors/C_human.cpp` movement leaves, `game/actors/C_player.cpp`
//! on-foot guards, `game/mission/C_recorder.cpp` cutscene states).
#![forbid(unsafe_code)]

use crate::human_actor::{HUMAN_ACTION_FALL, HUMAN_ACTION_STAND, HUMAN_ACTION_WALK};
use crate::human_move_coll::{
    movement_apply_gravity, movement_apply_step, movement_boost_small_step, StepContact,
    TEST_MOVE_COLL_BODY_LIFT, TEST_MOVE_COLL_HEAD_LIFT, TEST_MOVE_COLL_LEGS_LIFT,
    TEST_MOVE_COLL_RADII,
};

/// Host walk speed in m/s. Tuning, not a native constant.
pub const WALK_SPEED_M_S: f32 = 2.0;
/// Host run speed in m/s (held Shift). Tuning, not a native constant.
pub const RUN_SPEED_M_S: f32 = 4.5;
/// Host-tuned taxi destination: the on-foot leg is complete after the taxi
/// is driven this far. Tuning, not a native objective distance.
pub const TAXI_DESTINATION_M: f32 = 150.0;
/// Fixed simulation quantum in ms, matching the driving world's 16 ms tick.
pub const WALK_TICK_MS: u32 = 16;
/// Flat-world ground assumption is documented per call site; the scene height
/// probe stays open.
pub const WORLD_BOUND_RADIUS_M: f32 = 500.0;

/// Camera-relative wish direction on the ground plane plus run flag.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WalkInput {
    /// -1..1 along the camera forward (ground-projected).
    pub forward: f32,
    /// -1..1 to the camera right.
    pub strafe: f32,
    /// Held run key.
    pub run: bool,
}

impl WalkInput {
    /// Clamp every axis into -1..1; zero-length input stays put.
    pub fn clamped(self) -> Self {
        Self {
            forward: self.forward.clamp(-1.0, 1.0),
            strafe: self.strafe.clamp(-1.0, 1.0),
            run: self.run,
        }
    }

    /// True when any axis is meaningfully held.
    pub fn is_active(self) -> bool {
        self.forward.abs() > 0.01 || self.strafe.abs() > 0.01
    }
}

/// On-foot player state: world position, facing yaw and fall bookkeeping.
#[derive(Clone, Debug, PartialEq)]
pub struct OnFootPlayer {
    pos: [f32; 3],
    yaw: f32,
    fall: f32,
    action: i32,
    spawn: [f32; 3],
}

impl OnFootPlayer {
    /// Spawn at `pos` facing `yaw` radians around Y. The spawn point anchors
    /// the world-bound clamp and the taxi-stand offset.
    pub fn spawn(pos: [f32; 3], yaw: f32) -> Self {
        Self {
            pos,
            yaw,
            fall: 0.0,
            action: HUMAN_ACTION_STAND,
            spawn: pos,
        }
    }

    /// Current world position.
    pub fn pos(&self) -> [f32; 3] {
        self.pos
    }

    /// Current facing yaw in radians around Y.
    pub fn yaw(&self) -> f32 {
        self.yaw
    }

    /// Distance walked from the spawn point on the ground plane.
    pub fn distance_from_spawn(&self) -> f32 {
        let dx = self.pos[0] - self.spawn[0];
        let dz = self.pos[2] - self.spawn[2];
        dx.hypot(dz)
    }

    /// Advance one tick. `cam_yaw` is the camera orbit yaw; the wish direction
    /// is camera-relative. `ground_height` is the flat-world probe result
    /// (`None` means no ground: the player falls). `sweep` resolves the step
    /// against blockers and returns the contact (`None` means free flight).
    /// Wall sliding stays deferred; a blocked sweep simply stops the step.
    pub fn step(
        &mut self,
        dt_ms: u32,
        input: WalkInput,
        cam_yaw: f32,
        ground_height: Option<f32>,
        sweep: impl FnOnce([f32; 3], [f32; 3]) -> Option<StepContact>,
    ) -> StepOutcome {
        let dt = dt_ms.max(1) as f32 * 0.001;
        let input = input.clamped();
        let speed = if input.run {
            RUN_SPEED_M_S
        } else {
            WALK_SPEED_M_S
        };
        // Camera-relative wish direction on the ground plane.
        let (sin, cos) = cam_yaw.sin_cos();
        let mut wish = [
            (sin * input.forward + cos * input.strafe) * speed * dt,
            0.0,
            (cos * input.forward - sin * input.strafe) * speed * dt,
        ];
        if input.is_active() {
            self.yaw = wish[0].atan2(wish[2]);
            self.action = HUMAN_ACTION_WALK;
        } else {
            wish[0] = 0.0;
            wish[2] = 0.0;
            if ground_height.is_some() {
                self.action = HUMAN_ACTION_STAND;
            }
        }
        let grounded_probe = ground_height.is_some();
        let (displacement, fall) = movement_apply_gravity(wish, self.fall, dt_ms.max(1));
        self.fall = fall;
        if !grounded_probe {
            self.action = HUMAN_ACTION_FALL;
        }
        let displacement = movement_boost_small_step(self.action, displacement);
        let contact = sweep(self.pos, displacement);
        let resolved = movement_apply_step(
            self.action,
            self.pos,
            displacement,
            self.fall,
            false,
            true,
            grounded_probe,
            contact,
        );
        self.action = resolved.action_state;
        self.fall = resolved.fall;
        let mut pos = resolved.pos;
        if let Some(h) = ground_height {
            if pos[1] <= h {
                pos[1] = h;
                self.fall = 0.0;
                if self.action == HUMAN_ACTION_FALL {
                    self.action = HUMAN_ACTION_STAND;
                }
            }
        }
        // World-bound clamp on the ground plane; the spawn anchors it.
        let dx = pos[0] - self.spawn[0];
        let dz = pos[2] - self.spawn[2];
        let dist = dx.hypot(dz);
        if dist > WORLD_BOUND_RADIUS_M {
            let k = WORLD_BOUND_RADIUS_M / dist;
            pos[0] = self.spawn[0] + dx * k;
            pos[2] = self.spawn[2] + dz * k;
        }
        let moved = [
            pos[0] - self.pos[0],
            pos[1] - self.pos[1],
            pos[2] - self.pos[2],
        ];
        self.pos = pos;
        StepOutcome {
            moved,
            landed: resolved.landed,
            grounded: ground_height.is_some(),
            falling: self.action == HUMAN_ACTION_FALL,
        }
    }
}

/// What one walk tick did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepOutcome {
    /// Actual position delta this tick.
    pub moved: [f32; 3],
    /// The step resolution reported a landing.
    pub landed: bool,
    /// A ground probe result was supplied.
    pub grounded: bool,
    /// The player is in FALL after the tick.
    pub falling: bool,
}

/// Cutscene playback: timed lock that pauses walking and drives the
/// cinematic camera. Duration and content come from the caller; the recorded
/// actor binding stays with `CutsceneOwner` (DEFERRED).
#[derive(Clone, Debug, PartialEq)]
pub struct CutscenePlayback {
    duration_ms: u32,
    elapsed_ms: u32,
    playing: bool,
}

impl CutscenePlayback {
    /// Idle playback with no content staged.
    pub fn idle() -> Self {
        Self {
            duration_ms: 0,
            elapsed_ms: 0,
            playing: false,
        }
    }

    /// Start playing a `duration_ms` cutscene from zero. Zero duration is
    /// rejected: there is nothing to play.
    pub fn start(&mut self, duration_ms: u32) -> bool {
        if duration_ms == 0 {
            return false;
        }
        self.duration_ms = duration_ms;
        self.elapsed_ms = 0;
        self.playing = true;
        true
    }

    /// True while a cutscene holds the player locked.
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// 0..1 progress of the running cutscene; 0 while idle.
    pub fn progress(&self) -> f32 {
        if !self.playing || self.duration_ms == 0 {
            return 0.0;
        }
        (self.elapsed_ms as f32 / self.duration_ms as f32).clamp(0.0, 1.0)
    }

    /// Advance the clock. Returns true exactly on the tick that finishes.
    pub fn advance(&mut self, dt_ms: u32) -> bool {
        if !self.playing {
            return false;
        }
        self.elapsed_ms = self.elapsed_ms.saturating_add(dt_ms);
        if self.elapsed_ms >= self.duration_ms {
            self.playing = false;
            return true;
        }
        false
    }

    /// Abort playback; returns true when something was playing.
    pub fn stop(&mut self) -> bool {
        if !self.playing {
            return false;
        }
        self.playing = false;
        self.elapsed_ms = 0;
        true
    }
}

/// First-mission objective stages in native order of play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mise01Stage {
    /// Watch (or skip) the intro cutscene.
    WatchIntro,
    /// Walk from the spawn to the taxi stand.
    ReachTaxiStand,
    /// Board the taxi (drives the staged MISE01 taxi world).
    BoardTaxi,
    /// Drive the taxi to the destination; the odometer counts up.
    DrivingToDestination,
    /// Taxi leg done: the first mission is completable end to end.
    Complete,
}

/// MISE01 progress over host-tracked events. Positions are caller-supplied
/// world coordinates; no original spawn/taxi coordinates are invented here.
#[derive(Clone, Debug, PartialEq)]
pub struct Mise01Progress {
    stage: Mise01Stage,
    taxi_stand: Option<[f32; 3]>,
    taxi_radius_m: f32,
    destination: Option<([f32; 3], f32)>,
}

impl Mise01Progress {
    /// Fresh tracker waiting for the intro cutscene.
    pub fn fresh() -> Self {
        Self {
            stage: Mise01Stage::WatchIntro,
            taxi_stand: None,
            taxi_radius_m: 3.0,
            destination: None,
        }
    }

    /// Staged destination anchor, if any.
    pub fn destination(&self) -> Option<([f32; 3], f32)> {
        self.destination
    }

    /// Current stage.
    pub fn stage(&self) -> Mise01Stage {
        self.stage
    }

    /// Stage the taxi-stand marker once the host scene places it. Rejected
    /// after the on-foot leg moved past it.
    pub fn stage_taxi_stand(&mut self, pos: [f32; 3], radius_m: f32) -> bool {
        if self.stage != Mise01Stage::WatchIntro && self.stage != Mise01Stage::ReachTaxiStand {
            return false;
        }
        self.taxi_stand = Some(pos);
        self.taxi_radius_m = radius_m.max(0.5);
        if self.stage == Mise01Stage::WatchIntro {
            return true;
        }
        true
    }

    /// The intro cutscene finished (played or skipped).
    pub fn cutscene_done(&mut self) {
        if self.stage == Mise01Stage::WatchIntro {
            self.stage = Mise01Stage::ReachTaxiStand;
        }
    }

    /// Report the on-foot position each tick. Advances to `BoardTaxi` inside
    /// the stand radius; without a staged stand it never advances.
    pub fn player_moved(&mut self, pos: [f32; 3]) {
        if self.stage != Mise01Stage::ReachTaxiStand {
            return;
        }
        if let Some(stand) = self.taxi_stand {
            let dx = pos[0] - stand[0];
            let dz = pos[2] - stand[2];
            if dx.hypot(dz) <= self.taxi_radius_m {
                self.stage = Mise01Stage::BoardTaxi;
            }
        }
    }

    /// The player boarded the taxi; the driving leg runs in the staged MISE01
    /// taxi world (`--car-driving` opens the same world standalone).
    pub fn boarded_taxi(&mut self) {
        if self.stage == Mise01Stage::BoardTaxi {
            self.stage = Mise01Stage::DrivingToDestination;
        }
    }

    /// Report the taxi odometer in metres. Without a staged graph destination
    /// this host threshold completes the leg; with one, only the real
    /// location counts (see [`Mise01Progress::taxi_moved`]).
    pub fn drive_distance(&mut self, odometer_m: f32) {
        if self.destination.is_none()
            && self.stage == Mise01Stage::DrivingToDestination
            && odometer_m >= TAXI_DESTINATION_M
        {
            self.stage = Mise01Stage::Complete;
        }
    }

    /// Stage the real destination anchor and its acceptance radius. Only the
    /// driving leg admits it.
    pub fn stage_destination(&mut self, pos: [f32; 3], radius_m: f32) -> bool {
        if self.stage != Mise01Stage::DrivingToDestination {
            return false;
        }
        self.destination = Some((pos, radius_m.max(0.5)));
        true
    }

    /// Report the taxi position each tick. Within the staged radius the
    /// mission completes.
    pub fn taxi_moved(&mut self, pos: [f32; 3]) {
        if self.stage != Mise01Stage::DrivingToDestination {
            return;
        }
        if let Some((anchor, radius)) = self.destination {
            let dx = pos[0] - anchor[0];
            let dz = pos[2] - anchor[2];
            if dx.hypot(dz) <= radius {
                self.stage = Mise01Stage::Complete;
            }
        }
    }

    /// Short HUD line for the current stage.
    pub fn hud_line(&self) -> &'static str {
        match self.stage {
            Mise01Stage::WatchIntro => "MISE01: press C for the intro cutscene, Enter to skip it",
            Mise01Stage::ReachTaxiStand => "MISE01: walk (WASD, Shift runs) to the taxi stand",
            Mise01Stage::BoardTaxi => "MISE01: press E to board the taxi",
            Mise01Stage::DrivingToDestination => {
                "MISE01: drive to the road marker (W/S/A/D, Space brake)"
            }
            Mise01Stage::Complete => "MISE01 complete: Esc returns to the menu",
        }
    }
}

/// Axis-aligned bounding box as `[min, max]`.
pub type MeshAabb = [[f32; 3]; 2];

/// Bounding box of one vertex soup. `None` for empty or nonfinite input;
/// such meshes never collide.
pub fn mesh_aabb(verts: &[[f32; 3]]) -> Option<MeshAabb> {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    let mut count = 0u32;
    for v in verts {
        if !(v[0].is_finite() && v[1].is_finite() && v[2].is_finite()) {
            continue;
        }
        for a in 0..3 {
            min[a] = min[a].min(v[a]);
            max[a] = max[a].max(v[a]);
        }
        count += 1;
    }
    if count == 0 {
        return None;
    }
    Some([min, max])
}

/// Vertical-down ray against one triangle (Moller-Trumbore with the ray
/// direction fixed to `(0,-1,0)`). Returns the hit height when the `(x, z)`
/// point lands inside the triangle at or below `top_y`.
pub fn ray_down_tri(tri: &[[f32; 3]; 3], x: f32, z: f32, top_y: f32) -> Option<f32> {
    let [a, b, c] = *tri;
    for v in [a, b, c] {
        if !(v[0].is_finite() && v[1].is_finite() && v[2].is_finite()) {
            return None;
        }
    }
    let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    // dir x e2 for dir=(0,-1,0): (-e2[2], 0, e2[0]).
    let p = [-e2[2], 0.0, e2[0]];
    let det = e1[0] * p[0] + e1[1] * p[1] + e1[2] * p[2];
    if det.abs() < 1e-8 {
        return None;
    }
    let inv = 1.0 / det;
    let t = [x - a[0], top_y - a[1], z - a[2]];
    let u = (t[0] * p[0] + t[1] * p[1] + t[2] * p[2]) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    // q = t x e1; v = dir . q * inv = -q[1] * inv for dir=(0,-1,0).
    let q = [
        t[1] * e1[2] - t[2] * e1[1],
        t[2] * e1[0] - t[0] * e1[2],
        t[0] * e1[1] - t[1] * e1[0],
    ];
    let v = -q[1] * inv;
    if !(0.0 <= v && u + v <= 1.0) {
        return None;
    }
    let dist = (e2[0] * q[0] + e2[1] * q[1] + e2[2] * q[2]) * inv;
    // NaN-preserving compare: a NaN distance must reject.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    if !(dist >= 0.0) {
        return None;
    }
    Some(top_y - dist)
}

/// Ground height under `(x, z)`: the highest triangle hit at or below
/// `top_y` and no deeper than `max_drop`. Meshes are prefiltered by `boxes`;
/// triangle data is pulled per overlapping mesh through `tris_of` so the
/// space itself only retains bounding boxes.
pub fn ground_height_below(
    boxes: &[Option<MeshAabb>],
    tris_of: &dyn Fn(usize) -> Option<Vec<[[f32; 3]; 3]>>,
    x: f32,
    z: f32,
    top_y: f32,
    max_drop: f32,
) -> Option<f32> {
    if !(x.is_finite() && z.is_finite() && top_y.is_finite() && max_drop >= 0.0) {
        return None;
    }
    let mut best: Option<f32> = None;
    for (index, bb) in boxes.iter().enumerate() {
        let Some([[min_x, min_y, min_z], [max_x, max_y, max_z]]) = bb else {
            continue;
        };
        if x < *min_x || x > *max_x || z < *min_z || z > *max_z {
            continue;
        }
        if *min_y > top_y || *max_y < top_y - max_drop {
            continue;
        }
        let Some(tris) = tris_of(index) else {
            continue;
        };
        for tri in &tris {
            if let Some(hit) = ray_down_tri(tri, x, z, top_y) {
                if hit >= top_y - max_drop && best.is_none_or(|b| hit > b) {
                    best = Some(hit);
                }
            }
        }
    }
    best
}

/// Closest point on triangle `abc` to `p` (Real-Time Collision Detection
/// 5.1.5, plain f32 host math for the sweep below).
fn closest_point_tri(p: [f32; 3], a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let ap = [p[0] - a[0], p[1] - a[1], p[2] - a[2]];
    let dot = |u: [f32; 3], v: [f32; 3]| u[0] * v[0] + u[1] * v[1] + u[2] * v[2];
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = [p[0] - b[0], p[1] - b[1], p[2] - b[2]];
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return [a[0] + ab[0] * v, a[1] + ab[1] * v, a[2] + ab[2] * v];
    }
    let cp = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return [a[0] + ac[0] * w, a[1] + ac[1] * w, a[2] + ac[2] * w];
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return [
            b[0] + (c[0] - b[0]) * w,
            b[1] + (c[1] - b[1]) * w,
            b[2] + (c[2] - b[2]) * w,
        ];
    }
    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    [
        a[0] + ab[0] * v + ac[0] * w,
        a[1] + ab[1] * v + ac[1] * w,
        a[2] + ab[2] * v + ac[2] * w,
    ]
}

/// Squared distance from a sphere center to a triangle.
fn sphere_tri_dist_sq(center: [f32; 3], tri: &[[f32; 3]; 3]) -> f32 {
    let q = closest_point_tri(center, tri[0], tri[1], tri[2]);
    let d = [center[0] - q[0], center[1] - q[1], center[2] - q[2]];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
}

/// Evidenced standing capsule spheres as (height above feet, radius):
/// legs/body/head from `TEST_MOVE_COLL_RADII` and the `*_LIFT` words
/// (`C_human.cpp:5278-5281`; body/head lifts sit above the legs sphere).
pub const WALK_CAPSULE_SPHERES: [(f32, f32); 3] = [
    (TEST_MOVE_COLL_LEGS_LIFT, TEST_MOVE_COLL_RADII[0]),
    (
        TEST_MOVE_COLL_LEGS_LIFT + TEST_MOVE_COLL_BODY_LIFT,
        TEST_MOVE_COLL_RADII[1],
    ),
    (
        TEST_MOVE_COLL_LEGS_LIFT + TEST_MOVE_COLL_HEAD_LIFT,
        TEST_MOVE_COLL_RADII[2],
    ),
];

/// Wall sweep over real scene geometry: the evidenced capsule walks `delta`
/// (ground plane) from `feet` in discrete substeps. The second return is
/// whether any wall contact happened: a completed slide still reports
/// contact so callers route the rest position through the swept branch
/// instead of free flight. One slide bounce: the remaining motion is
/// projected onto the contact plane and re-walked once; corners therefore
/// stop the step. The native swept TestMoveColl arms stay deferred; radii
/// and lifts are native words, the bounce is an explicit host driver.
pub fn sweep_capsule(
    boxes: &[Option<MeshAabb>],
    tris_of: &dyn Fn(usize) -> Option<Vec<[[f32; 3]; 3]>>,
    feet: [f32; 3],
    delta: [f32; 3],
) -> ([f32; 3], bool) {
    if !feet.iter().chain(delta.iter()).all(|v| v.is_finite()) {
        return (feet, true);
    }
    let dist = delta[0].hypot(delta[2]);
    if dist <= 0.0 {
        return (feet, false);
    }
    let (rest, blocked, normal) = sweep_until(boxes, tris_of, feet, delta);
    if !blocked {
        return (rest, false);
    }
    let Some(mut n) = normal else {
        return (rest, true);
    };
    // Orient the plane normal against the motion.
    if n[0] * delta[0] + n[2] * delta[2] > 0.0 {
        n = [-n[0], n[1], -n[2]];
    }
    // Project the leftover onto the wall plane (single bounce).
    let travelled = [rest[0] - feet[0], rest[2] - feet[2]];
    let leftover = [delta[0] - travelled[0], delta[2] - travelled[1]];
    let into = leftover[0] * n[0] + leftover[1] * n[2];
    let slide = [leftover[0] - n[0] * into, leftover[1] - n[2] * into];
    if slide[0].hypot(slide[1]) < 1e-4 {
        return (rest, true);
    }
    let (rest2, _, _) = sweep_until(boxes, tris_of, rest, [slide[0], 0.0, slide[1]]);
    (rest2, true)
}

/// Walk `delta` in substeps; stop at the first penetration and report the
/// contact plane normal of the blocking triangle, if any.
fn sweep_until(
    boxes: &[Option<MeshAabb>],
    tris_of: &dyn Fn(usize) -> Option<Vec<[[f32; 3]; 3]>>,
    feet: [f32; 3],
    delta: [f32; 3],
) -> ([f32; 3], bool, Option<[f32; 3]>) {
    let dist = delta[0].hypot(delta[2]);
    if dist <= 0.0 {
        return (feet, false, None);
    }
    let steps = ((dist / 0.1).ceil() as u32).clamp(1, 16);
    let mut rest = feet;
    for step in 1..=steps {
        let k = step as f32 / steps as f32;
        let candidate = [feet[0] + delta[0] * k, feet[1], feet[2] + delta[2] * k];
        if let Some(normal) = capsule_contact_normal(boxes, tris_of, candidate) {
            return (rest, true, Some(normal));
        }
        rest = candidate;
    }
    (rest, false, None)
}

/// First penetrating contact normal at `feet`, if any capsule sphere touches
/// scene geometry. The normal comes from the blocking triangle, normalized.
fn capsule_contact_normal(
    boxes: &[Option<MeshAabb>],
    tris_of: &dyn Fn(usize) -> Option<Vec<[[f32; 3]; 3]>>,
    feet: [f32; 3],
) -> Option<[f32; 3]> {
    let pad = TEST_MOVE_COLL_RADII[0];
    for (index, bb) in boxes.iter().enumerate() {
        let Some([[min_x, min_y, min_z], [max_x, max_y, max_z]]) = bb else {
            continue;
        };
        // The capsule spans feet.y..feet.y+1.8; expand by the largest radius.
        if feet[0] + pad < *min_x || feet[0] - pad > *max_x {
            continue;
        }
        if feet[2] + pad < *min_z || feet[2] - pad > *max_z {
            continue;
        }
        if feet[1] + 1.8 + pad < *min_y || feet[1] - pad > *max_y {
            continue;
        }
        let Some(tris) = tris_of(index) else {
            continue;
        };
        for tri in &tris {
            if !tri.iter().flatten().all(|v| v.is_finite()) {
                continue;
            }
            for (lift, radius) in WALK_CAPSULE_SPHERES {
                let center = [feet[0], feet[1] + lift, feet[2]];
                if sphere_tri_dist_sq(center, tri) < radius * radius {
                    return tri_normal(tri);
                }
            }
        }
    }
    None
}

/// Unit face normal of a triangle, or `None` for a degenerate one.
fn tri_normal(tri: &[[f32; 3]; 3]) -> Option<[f32; 3]> {
    let e1 = [
        tri[1][0] - tri[0][0],
        tri[1][1] - tri[0][1],
        tri[1][2] - tri[0][2],
    ];
    let e2 = [
        tri[2][0] - tri[0][0],
        tri[2][1] - tri[0][1],
        tri[2][2] - tri[0][2],
    ];
    let n = [
        e1[1] * e2[2] - e1[2] * e2[1],
        e1[2] * e2[0] - e1[0] * e2[2],
        e1[0] * e2[1] - e1[1] * e2[0],
    ];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len < 1e-8 {
        None
    } else {
        Some([n[0] / len, n[1] / len, n[2] / len])
    }
}

/// Precomputed per-mesh bounding boxes of one static scene. Only boxes are
/// retained; triangles stay with the scene and are pulled per overlap.
#[derive(Clone, Debug, Default)]
pub struct MeshGroundSpace {
    boxes: Vec<Option<MeshAabb>>,
}

impl MeshGroundSpace {
    /// Snapshot the bounding boxes of `count` meshes through `aabb_of`.
    /// Meshes beyond `count` (e.g. later-merged car draws) are ignored.
    pub fn build(count: usize, aabb_of: &dyn Fn(usize) -> Option<MeshAabb>) -> Self {
        Self {
            boxes: (0..count).map(aabb_of).collect(),
        }
    }

    /// Mesh count covered by this space.
    pub fn len(&self) -> usize {
        self.boxes.len()
    }

    /// True when no mesh was admitted.
    pub fn is_empty(&self) -> bool {
        self.boxes.is_empty()
    }

    /// Highest real-geometry ground under `(x, z)`, or `None` over the void.
    pub fn ground_below(
        &self,
        tris_of: &dyn Fn(usize) -> Option<Vec<[[f32; 3]; 3]>>,
        x: f32,
        z: f32,
        top_y: f32,
        max_drop: f32,
    ) -> Option<f32> {
        ground_height_below(&self.boxes, tris_of, x, z, top_y, max_drop)
    }

    /// Wall sweep from `feet` along ground-plane `delta` against the retained
    /// boxes. See [`sweep_capsule`].
    pub fn sweep_walls(
        &self,
        tris_of: &dyn Fn(usize) -> Option<Vec<[[f32; 3]; 3]>>,
        feet: [f32; 3],
        delta: [f32; 3],
    ) -> ([f32; 3], bool) {
        sweep_capsule(&self.boxes, tris_of, feet, delta)
    }
}

/// Silent host audio probe for cutscene dubbing ticks: no voice is staged,
/// so nothing logically plays and every position/duration reads zero.
/// Real `DabingTick` still advances channel clocks, subtitle fades and
/// morphs; device mixing stays deferred.
#[derive(Clone, Copy, Debug, Default)]
pub struct SilentDabingAudio;

impl crate::dabing::DabingAudioProbe for SilentDabingAudio {
    fn dabing_logical_playing(&self, _sound: &crate::dabing::SoundId) -> bool {
        false
    }
    fn dabing_position_ms(&self, _sound: &crate::dabing::SoundId) -> u32 {
        0
    }
    fn dabing_duration_ms(&self, _sound: &crate::dabing::SoundId) -> u32 {
        0
    }
    fn dabing_release(&self, _sound: &crate::dabing::SoundId) {}
}

/// Recording host face sink for cutscene dubbing ticks: every weight write
/// lands here in call order for evidence; no model is posed.
#[derive(Clone, Debug, Default)]
pub struct FaceWeightLog {
    writes: Vec<(crate::dabing::DabingFaceId, u32, u32, f32)>,
}

impl FaceWeightLog {
    /// Recorded `(face, region, level, weight)` writes in call order.
    pub fn writes(&self) -> &[(crate::dabing::DabingFaceId, u32, u32, f32)] {
        &self.writes
    }
}

impl crate::dabing::DabingFaceSink for FaceWeightLog {
    fn set_face_weight(
        &mut self,
        face: crate::dabing::DabingFaceId,
        region: u32,
        level: u32,
        weight: f32,
    ) {
        self.writes.push((face, region, level, weight));
    }
}

/// Real player/taxi spawn positions from mission scene2 actor metadata
/// (`TommyTAXIDRIVER` kind 2, `taxi` kind 4 with their object world
/// positions). No coordinates are invented; both must be present.
pub fn mission_actor_spawn(
    installation: &lhr_assets::Installation,
    mission: &str,
) -> Result<([f32; 3], [f32; 3]), String> {
    let path = format!("missions/{mission}/scene2.bin");
    let bytes = installation
        .read_optional(&path)
        .map_err(|e| format!("Mission actor metadata read stopped: {e}"))?
        .ok_or_else(|| format!("Mission actor metadata absent: {path}"))?;
    let scene = lhr_assets::parse_scene2(&bytes)
        .map_err(|e| format!("Mission actor metadata parse stopped: {e:?}"))?;
    let actors = crate::mission_world_completeness::resolve_mission_actors(&scene);
    match (actors.player_world_position, actors.taxi_world_position) {
        (Some(player), Some(taxi)) => Ok((player, taxi)),
        _ => Err(format!(
            "Mission actor metadata incomplete: player_found={} taxi_found={}",
            actors.player_found, actors.taxi_found
        )),
    }
}

/// Mission end-detector position from scene2 metadata (`detector_konec`,
/// kind 5 with its object world position). The name attests the drive's
/// end in the mission's own data ("konec" = end); the exact script
/// linkage (which program waits on it) is unverified because GameInit
/// scheduling stops at the CREATE frontier, so this stays a documented
/// host anchor, not a script claim. Requires exactly one matching record
/// with a known position; anything else reports instead of inventing.
pub fn mission_end_detector(
    installation: &lhr_assets::Installation,
    mission: &str,
) -> Result<[f32; 3], String> {
    let path = format!("missions/{mission}/scene2.bin");
    let bytes = installation
        .read_optional(&path)
        .map_err(|e| format!("Mission actor metadata read stopped: {e}"))?
        .ok_or_else(|| format!("Mission actor metadata absent: {path}"))?;
    let scene = lhr_assets::parse_scene2(&bytes)
        .map_err(|e| format!("Mission actor metadata parse stopped: {e:?}"))?;
    let standings = crate::mission_world_completeness::mission_detector_standings(&scene);
    let mut matches = standings
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("detector_konec"));
    match (matches.next(), matches.next()) {
        (Some((_, Some(position))), None) => Ok(*position),
        (Some(_), None) => Err("Mission end detector has no position: detector_konec".into()),
        (None, _) => Err("Mission end detector absent: no detector_konec".into()),
        _ => Err("Mission end detector ambiguous: duplicate detector_konec".into()),
    }
}

/// Additive Dijkstra penalty for a graph node near learned trap poses.
/// Full [`TRAP_PENALTY_M`] at the trap itself, linearly falling to zero
/// at [`TRAP_RADIUS_M`]. Host-side routing cost only; it changes no
/// native behavior and claims no original function.
pub const TRAP_RADIUS_M: f32 = 45.0;
/// A single disc crossing costs at most one full penalty. Raised
/// 2000 -> 6000 and reverted: the 6000 detour (probe_pen6k, 14223 m)
/// kept the identical 19.4 m min-distance crossing as the 7622 m
/// route, proving the [-1318,153] rail a cut-vertex no finite penalty
/// routes around — while the doubled route no longer fits the tick
/// budget. So the rail must be DRIVEN through (rail-carry hold),
/// not avoided.
pub const TRAP_PENALTY_M: f32 = 2000.0;

/// Sum of trap penalties for a road-graph node position. Returns `0.0`
/// with no traps or beyond the radius, so unpenalized routing is the
/// exact fallback when no trap is near.
pub fn trap_penalty(node: [f32; 3], traps: &[[f32; 3]]) -> f32 {
    trap_penalty_with_radius(node, traps, TRAP_RADIUS_M)
}

/// Radius-parameterized node penalty. The 45 m default was never
/// ablated (only the 2000/6000 penalty height was); furniture pockets
/// are metre-scale, so overlapping 45 m discs can merge into a
/// street-covering blob that disconnects the graph by host choice
/// rather than by geometry. The probe's --trap-radius flag ablates
/// this dimension; the const stays the default.
pub fn trap_penalty_with_radius(node: [f32; 3], traps: &[[f32; 3]], radius: f32) -> f32 {
    let mut penalty = 0.0f32;
    // NaN-preserving compare: a NaN radius adds no penalty.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    if !(radius > 0.0) {
        return penalty;
    }
    for t in traps {
        let d = (node[0] - t[0]).hypot(node[2] - t[2]);
        if d < radius {
            penalty += TRAP_PENALTY_M * (1.0 - d / radius);
        }
    }
    penalty
}

/// Full additive penalty for a Dijkstra edge `a -> b`: the maximum of
/// the endpoint penalties plus a segment term, so a long straight cut
/// whose endpoints both lie outside the trap radius still pays when
/// its middle passes through the pocket. Keeps routing honest on
/// coarse graphs where subdivided waypoints thread between nodes.
pub fn trap_edge_penalty(a: [f32; 3], b: [f32; 3], traps: &[[f32; 3]]) -> f32 {
    trap_edge_penalty_with_radius(a, b, traps, TRAP_RADIUS_M)
}

/// Radius-parameterized edge penalty: the maximum of the endpoint
/// penalties plus a segment term, all measured against `radius`.
/// See [`trap_penalty_with_radius`].
pub fn trap_edge_penalty_with_radius(
    a: [f32; 3],
    b: [f32; 3],
    traps: &[[f32; 3]],
    radius: f32,
) -> f32 {
    let mut penalty =
        trap_penalty_with_radius(a, traps, radius).max(trap_penalty_with_radius(b, traps, radius));
    // NaN-preserving compare: a NaN radius adds no penalty.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    if !(radius > 0.0) {
        return penalty;
    }
    for t in traps {
        let abx = b[0] - a[0];
        let abz = b[2] - a[2];
        let denom = abx * abx + abz * abz;
        if denom <= 0.0 {
            continue;
        }
        let mut s = ((t[0] - a[0]) * abx + (t[2] - a[2]) * abz) / denom;
        s = s.clamp(0.0, 1.0);
        let cx = a[0] + abx * s - t[0];
        let cz = a[2] + abz * s - t[2];
        let d = (cx * cx + cz * cz).sqrt();
        if d < radius {
            let p = TRAP_PENALTY_M * (1.0 - d / radius);
            if p > penalty {
                penalty = p;
            }
        }
    }
    penalty
}

/// Road-graph edge enumerator: fills `(neighbor, weight)` pairs for a node.
pub type RoadNeighbors<'a> = &'a dyn Fn(usize, &mut Vec<(usize, f32)>);

/// Full Dijkstra distances from the node nearest `from` over `neighbors`
/// edge weights. Returns the start node and per-node distances (`INFINITY`
/// stays unreached). The search itself is an explicit host driver.
pub fn road_graph_distances(
    node_pos: &dyn Fn(usize) -> Option<[f32; 3]>,
    neighbors: RoadNeighbors<'_>,
    node_count: usize,
    from: [f32; 3],
) -> (Option<usize>, Vec<f32>) {
    let (start, dist, _) = dijkstra_core(node_pos, neighbors, node_count, from);
    (start, dist)
}

/// Shared Dijkstra core with predecessor tracking. `prev[i]` holds the
/// previous node on the current best path from the start, if any.
fn dijkstra_core(
    node_pos: &dyn Fn(usize) -> Option<[f32; 3]>,
    neighbors: RoadNeighbors<'_>,
    node_count: usize,
    from: [f32; 3],
) -> (Option<usize>, Vec<f32>, Vec<Option<usize>>) {
    if node_count == 0 || !from.iter().all(|v| v.is_finite()) {
        return (None, Vec::new(), Vec::new());
    }
    let mut start = None;
    let mut best = f32::INFINITY;
    for i in 0..node_count {
        let Some(p) = node_pos(i) else {
            continue;
        };
        let d = (p[0] - from[0]).hypot(p[2] - from[2]);
        if d < best {
            best = d;
            start = Some(i);
        }
    }
    let Some(start) = start else {
        return (None, Vec::new(), Vec::new());
    };
    let mut dist = vec![f32::INFINITY; node_count];
    let mut prev = vec![None; node_count];
    let mut visited = vec![false; node_count];
    dist[start] = 0.0;
    let mut edges = Vec::new();
    for _ in 0..node_count {
        let mut next = None;
        let mut next_dist = f32::INFINITY;
        for (i, &d) in dist.iter().enumerate() {
            if !visited[i] && d < next_dist {
                next_dist = d;
                next = Some(i);
            }
        }
        let Some(current) = next else {
            break;
        };
        visited[current] = true;
        edges.clear();
        neighbors(current, &mut edges);
        for (other, weight) in edges.drain(..) {
            if other < node_count && !visited[other] && weight.is_finite() && weight >= 0.0 {
                let candidate = next_dist + weight;
                if candidate < dist[other] {
                    dist[other] = candidate;
                    prev[other] = Some(current);
                }
            }
        }
    }
    (Some(start), dist, prev)
}

/// Pick a real destination on a road graph: Dijkstra from the node nearest
/// `from`, returning the nearest node (by graph distance) at least
/// `min_dist_m` away with its position and distance. Positions beyond `from`
/// are real mission data; the search itself is an explicit host driver.
pub fn pick_road_destination(
    node_pos: &dyn Fn(usize) -> Option<[f32; 3]>,
    neighbors: RoadNeighbors<'_>,
    node_count: usize,
    from: [f32; 3],
    min_dist_m: f32,
) -> Option<(usize, [f32; 3], f32)> {
    if min_dist_m <= 0.0 {
        return None;
    }
    let (start, dist) = road_graph_distances(node_pos, neighbors, node_count, from);
    start?;
    let mut order: Vec<usize> = (0..node_count).collect();
    order.sort_by(|&a, &b| {
        dist[a]
            .partial_cmp(&dist[b])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for i in order {
        if dist[i] >= min_dist_m {
            if let Some(p) = node_pos(i) {
                return Some((i, p, dist[i]));
            }
        }
    }
    None
}

/// Dijkstra index path from the node nearest `from` to `dest` over
/// `neighbors` edge weights, in drive order with its graph distance.
/// Returns `None` for an invalid or unreached destination, or when no
/// node has a position. The search itself is an explicit host driver.
pub fn road_graph_path(
    node_pos: &dyn Fn(usize) -> Option<[f32; 3]>,
    neighbors: RoadNeighbors<'_>,
    node_count: usize,
    from: [f32; 3],
    dest: usize,
) -> Option<(Vec<usize>, f32)> {
    if dest >= node_count || node_pos(dest).is_none() {
        return None;
    }
    let (start, dist, prev) = dijkstra_core(node_pos, neighbors, node_count, from);
    let start = start?;
    if !dist[dest].is_finite() {
        return None;
    }
    let mut reversed = vec![dest];
    let mut current = dest;
    let mut guard = node_count + 1;
    while current != start {
        guard = guard.saturating_sub(1);
        if guard == 0 {
            return None;
        }
        current = prev[current]?;
        reversed.push(current);
    }
    reversed.reverse();
    Some((reversed, dist[dest]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn free_flight(from: [f32; 3], _delta: [f32; 3]) -> Option<StepContact> {
        let _ = from;
        None
    }

    fn input(forward: f32, strafe: f32, run: bool) -> WalkInput {
        WalkInput {
            forward,
            strafe,
            run,
        }
    }

    #[test]
    fn walk_moves_forward_at_walk_speed() {
        let mut player = OnFootPlayer::spawn([0.0, 0.0, 0.0], 0.0);
        let outcome = player.step(1000, input(1.0, 0.0, false), 0.0, Some(0.0), free_flight);
        assert!(outcome.grounded);
        assert!(!outcome.falling);
        assert!((player.pos()[2] - WALK_SPEED_M_S).abs() < 1e-4);
        assert!((player.pos()[0]).abs() < 1e-4);
    }

    #[test]
    fn run_is_faster_than_walk() {
        let mut walk = OnFootPlayer::spawn([0.0, 0.0, 0.0], 0.0);
        walk.step(1000, input(1.0, 0.0, false), 0.0, Some(0.0), free_flight);
        let mut run = OnFootPlayer::spawn([0.0, 0.0, 0.0], 0.0);
        run.step(1000, input(1.0, 0.0, true), 0.0, Some(0.0), free_flight);
        assert!(run.pos()[2] > walk.pos()[2]);
        assert!((run.pos()[2] - RUN_SPEED_M_S).abs() < 1e-4);
    }

    #[test]
    fn strafe_moves_sideways_and_turns_to_face_motion() {
        let mut player = OnFootPlayer::spawn([0.0, 0.0, 0.0], 0.0);
        player.step(1000, input(0.0, 1.0, false), 0.0, Some(0.0), free_flight);
        assert!((player.pos()[0] - WALK_SPEED_M_S).abs() < 1e-4);
        assert!(player.yaw().abs() > 0.1);
    }

    #[test]
    fn blocked_step_stops_at_the_wall() {
        let mut player = OnFootPlayer::spawn([0.0, 0.0, 0.0], 0.0);
        let outcome = player.step(
            1000,
            input(1.0, 0.0, false),
            0.0,
            Some(0.0),
            |from, _delta| {
                Some(StepContact {
                    newpos: from,
                    material: 0,
                    owner_arm: false,
                })
            },
        );
        assert!(outcome.moved[2].abs() < 1e-4);
        assert!(player.distance_from_spawn() < 1e-4);
    }

    #[test]
    fn no_ground_falls_and_flat_ground_lands() {
        let mut player = OnFootPlayer::spawn([0.0, 10.0, 0.0], 0.0);
        let outcome = player.step(1000, input(0.0, 0.0, false), 0.0, None, free_flight);
        assert!(!outcome.grounded);
        assert!(player.pos()[1] < 10.0);
        let mut grounded = OnFootPlayer::spawn([0.0, 0.5, 0.0], 0.0);
        let landed = grounded.step(1000, input(0.0, 0.0, false), 0.0, Some(0.0), free_flight);
        assert!(landed.grounded);
        assert!((grounded.pos()[1] - 0.0).abs() < 1e-4);
    }

    #[test]
    fn world_bound_clamps_far_walks() {
        let mut player = OnFootPlayer::spawn([0.0, 0.0, 0.0], 0.0);
        for _ in 0..600 {
            player.step(1000, input(1.0, 0.0, true), 0.0, Some(0.0), free_flight);
        }
        assert!(player.distance_from_spawn() <= WORLD_BOUND_RADIUS_M + 1e-3);
    }

    #[test]
    fn cutscene_advance_finishes_exactly_once() {
        let mut cut = CutscenePlayback::idle();
        assert!(!cut.is_playing());
        assert!(!cut.start(0));
        assert!(cut.start(100));
        assert!(cut.is_playing());
        assert!(!cut.advance(40));
        assert!(cut.progress() > 0.0);
        assert!(cut.advance(60));
        assert!(!cut.is_playing());
        assert!(!cut.advance(60));
    }

    #[test]
    fn cutscene_stop_aborts_without_finishing() {
        let mut cut = CutscenePlayback::idle();
        assert!(cut.start(1000));
        assert!(!cut.advance(100));
        assert!(cut.stop());
        assert!(!cut.is_playing());
        assert!(!cut.stop());
    }

    #[test]
    fn objectives_advance_in_order_only() {
        let mut progress = Mise01Progress::fresh();
        assert_eq!(progress.stage(), Mise01Stage::WatchIntro);
        // Walking early does nothing; boarding early does nothing.
        progress.player_moved([0.0, 0.0, 0.0]);
        progress.boarded_taxi();
        assert_eq!(progress.stage(), Mise01Stage::WatchIntro);
        progress.cutscene_done();
        assert_eq!(progress.stage(), Mise01Stage::ReachTaxiStand);
        // No stand staged yet: walking anywhere does nothing.
        progress.player_moved([100.0, 0.0, 100.0]);
        assert_eq!(progress.stage(), Mise01Stage::ReachTaxiStand);
        assert!(progress.stage_taxi_stand([10.0, 0.0, 0.0], 3.0));
        progress.player_moved([0.0, 0.0, 0.0]);
        assert_eq!(progress.stage(), Mise01Stage::ReachTaxiStand);
        progress.player_moved([10.0, 0.0, 1.0]);
        assert_eq!(progress.stage(), Mise01Stage::BoardTaxi);
        assert_eq!(progress.hud_line(), "MISE01: press E to board the taxi");
        progress.boarded_taxi();
        assert_eq!(progress.stage(), Mise01Stage::DrivingToDestination);
        // No destination staged: the odometer threshold stands in.
        progress.taxi_moved([500.0, 0.0, 500.0]);
        assert_eq!(progress.stage(), Mise01Stage::DrivingToDestination);
        progress.drive_distance(10.0);
        assert_eq!(progress.stage(), Mise01Stage::DrivingToDestination);
        progress.drive_distance(TAXI_DESTINATION_M);
        assert_eq!(progress.stage(), Mise01Stage::Complete);
        // A staged graph destination wins over the odometer instead.
        let mut located = Mise01Progress::fresh();
        located.cutscene_done();
        assert!(located.stage_taxi_stand([10.0, 0.0, 0.0], 3.0));
        located.player_moved([10.0, 0.0, 1.0]);
        located.boarded_taxi();
        assert!(located.stage_destination([500.0, 0.0, 500.0], 3.0));
        assert_eq!(located.destination(), Some(([500.0, 0.0, 500.0], 3.0)));
        located.taxi_moved([0.0, 0.0, 0.0]);
        assert_eq!(located.stage(), Mise01Stage::DrivingToDestination);
        located.drive_distance(10_000.0);
        assert_eq!(located.stage(), Mise01Stage::DrivingToDestination);
        located.taxi_moved([501.0, 0.0, 500.0]);
        assert_eq!(located.stage(), Mise01Stage::Complete);
        // Completed stages never regress.
        progress.cutscene_done();
        progress.player_moved([0.0, 0.0, 0.0]);
        progress.taxi_moved([0.0, 0.0, 0.0]);
        assert_eq!(progress.stage(), Mise01Stage::Complete);
    }

    #[test]
    fn walk_input_clamps_and_detects_activity() {
        let held = input(5.0, -5.0, true).clamped();
        assert_eq!((held.forward, held.strafe), (1.0, -1.0));
        assert!(held.is_active());
        assert!(!input(0.0, 0.0, false).is_active());
    }

    #[test]
    fn mesh_aabb_skips_empty_and_nonfinite() {
        assert_eq!(mesh_aabb(&[]), None);
        assert_eq!(mesh_aabb(&[[f32::NAN; 3]]), None);
        assert_eq!(
            mesh_aabb(&[[1.0, 2.0, 3.0], [-1.0, 0.5, 4.0]]),
            Some([[-1.0, 0.5, 3.0], [1.0, 2.0, 4.0]])
        );
    }

    #[test]
    fn ray_down_hits_floor_and_misses_hole() {
        // Two triangles forming the unit square at y=0, x/z in [0,1].
        let floor = [
            [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            [[1.0, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 0.0, 1.0]],
        ];
        assert_eq!(ray_down_tri(&floor[0], 0.25, 0.25, 2.0), Some(0.0));
        assert_eq!(ray_down_tri(&floor[1], 0.75, 0.75, 2.0), Some(0.0));
        assert_eq!(ray_down_tri(&floor[0], 0.75, 0.75, 2.0), None);
        assert_eq!(ray_down_tri(&floor[1], 0.25, 0.25, 2.0), None);
        // Outside the square: no hit.
        assert_eq!(ray_down_tri(&floor[0], 2.0, 0.25, 2.0), None);
        // Below the floor: the ray only looks down.
        assert_eq!(ray_down_tri(&floor[0], 0.25, 0.25, -1.0), None);
        // Degenerate triangle: no hit.
        let flat = [[[0.0, 0.0, 0.0]; 3]];
        assert_eq!(ray_down_tri(&flat[0], 0.0, 0.0, 1.0), None);
    }

    #[test]
    fn ground_probe_prefilters_boxes_and_depth() {
        let boxes = vec![
            Some([[0.0, 0.0, 0.0], [1.0, 0.0, 1.0]]),
            Some([[5.0, 0.0, 5.0], [6.0, 0.0, 6.0]]),
            None,
        ];
        let tris = |index: usize| {
            if index == 0 {
                Some(vec![[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]])
            } else {
                None
            }
        };
        assert_eq!(
            ground_height_below(&boxes, &tris, 0.5, 0.5, 2.0, 4.0),
            Some(0.0)
        );
        // Wrong cell: no hit even though a box exists there without triangles.
        assert_eq!(ground_height_below(&boxes, &tris, 5.5, 5.5, 2.0, 4.0), None);
        // Too deep below the top: rejected by max_drop.
        assert_eq!(
            ground_height_below(&boxes, &tris, 0.5, 0.5, 10.0, 4.0),
            None
        );
    }

    #[test]
    fn closest_point_prefers_face_edge_and_vertex() {
        let tri = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        // Above the face.
        let q = closest_point_tri([0.25, 2.0, 0.25], tri[0], tri[1], tri[2]);
        assert!((q[0] - 0.25).abs() < 1e-6 && q[1].abs() < 1e-6 && (q[2] - 0.25).abs() < 1e-6);
        // Past vertex a.
        let q = closest_point_tri([-1.0, 0.0, -1.0], tri[0], tri[1], tri[2]);
        assert_eq!(q, tri[0]);
        // Past edge b-c (x + z = 2 line side).
        let q = closest_point_tri([1.0, 0.0, 1.0], tri[0], tri[1], tri[2]);
        assert!((q[0] + q[2] - 1.0).abs() < 1e-6);
        // Distance from the face point is the height.
        assert!((sphere_tri_dist_sq([0.25, 3.0, 0.25], &tri) - 9.0).abs() < 1e-4);
    }

    #[test]
    fn sweep_stops_at_wall_and_passes_free_flight() {
        // Vertical wall at x=2 spanning y 0..3, z -1..1.
        let wall = [
            [[2.0, 0.0, -1.0], [2.0, 3.0, -1.0], [2.0, 0.0, 1.0]],
            [[2.0, 3.0, -1.0], [2.0, 3.0, 1.0], [2.0, 0.0, 1.0]],
        ];
        let boxes = vec![mesh_aabb(&wall[0]), mesh_aabb(&wall[1])];
        let tris = |i: usize| Some(vec![wall[i]]);
        // Free flight parallel to the wall.
        let (rest, blocked) = sweep_capsule(&boxes, &tris, [0.0, 0.0, 0.0], [0.0, 0.0, 5.0]);
        assert!(!blocked);
        assert!((rest[2] - 5.0).abs() < 1e-4);
        // Head-on: the legs sphere (r=0.4 at +0.405) stops at x=1.6.
        let (rest, blocked) = sweep_capsule(&boxes, &tris, [0.0, 0.0, 0.0], [5.0, 0.0, 0.0]);
        assert!(blocked);
        assert!((rest[0] - 1.6).abs() < 0.11);
        assert!(rest[0] < 2.0);
    }

    #[test]
    fn sweep_substeps_catch_fast_thin_walls() {
        // Thin wall at x=1, only 0.02 thick: a 5 m step must not tunnel.
        let wall = [[[1.0, 0.0, -5.0], [1.0, 3.0, -5.0], [1.0, 0.0, 5.0]]];
        let boxes = vec![mesh_aabb(&wall[0])];
        let tris = |_: usize| Some(wall.to_vec());
        let (rest, blocked) = sweep_capsule(&boxes, &tris, [0.0, 0.0, 0.0], [5.0, 0.0, 0.0]);
        assert!(blocked);
        assert!(rest[0] < 1.0);
        // Nonfinite input never moves.
        let (rest, blocked) = sweep_capsule(&boxes, &tris, [f32::NAN, 0.0, 0.0], [5.0, 0.0, 0.0]);
        assert!(blocked);
        assert!(rest[0].is_nan());
    }

    #[test]
    fn sweep_slides_along_wall_and_stops_in_corners() {
        // Vertical wall at x=2 spanning y 0..3, z -5..5.
        let wall = [
            [[2.0, 0.0, -5.0], [2.0, 3.0, -5.0], [2.0, 0.0, 5.0]],
            [[2.0, 3.0, -5.0], [2.0, 3.0, 5.0], [2.0, 0.0, 5.0]],
        ];
        let boxes = vec![mesh_aabb(&wall[0]), mesh_aabb(&wall[1])];
        let tris = |i: usize| Some(vec![wall[i]]);
        // Diagonal approach: x stops at the wall, z keeps sliding; contact
        // is reported even though the slide completes.
        let (rest, contacted) = sweep_capsule(&boxes, &tris, [0.0, 0.0, 0.0], [5.0, 0.0, 3.0]);
        assert!(contacted);
        assert!(rest[0] < 2.0);
        assert!(rest[2] > 2.0);
        // Corner: a second wall at z=3 blocks the slide as well.
        let corner = [
            [[-5.0, 0.0, 3.0], [-5.0, 3.0, 3.0], [5.0, 0.0, 3.0]],
            [[-5.0, 3.0, 3.0], [5.0, 3.0, 3.0], [5.0, 0.0, 3.0]],
        ];
        let boxes2 = vec![
            mesh_aabb(&wall[0]),
            mesh_aabb(&wall[1]),
            mesh_aabb(&corner[0]),
            mesh_aabb(&corner[1]),
        ];
        let tris2 = |i: usize| {
            if i < 2 {
                Some(vec![wall[i]])
            } else {
                Some(vec![corner[i - 2]])
            }
        };
        let (rest, contacted) = sweep_capsule(&boxes2, &tris2, [0.0, 0.0, 0.0], [5.0, 0.0, 5.0]);
        assert!(contacted);
        assert!(rest[0] < 2.0 && rest[2] < 3.0);
    }

    #[test]
    fn road_destination_follows_graph_distance() {
        // Chain 0-1-2-3 spaced 60 m apart; 3 is 180 m by graph.
        let pos = |i: usize| Some([i as f32 * 60.0, 0.0, 0.0]);
        let link = |node: usize, out: &mut Vec<(usize, f32)>| {
            if node > 0 {
                out.push((node - 1, 60.0));
            }
            if node < 3 {
                out.push((node + 1, 60.0));
            }
        };
        let found = pick_road_destination(&pos, &link, 4, [5.0, 0.0, 0.0], 150.0);
        assert_eq!(found, Some((3, [180.0, 0.0, 0.0], 180.0)));
        // Too far: unreachable.
        assert_eq!(
            pick_road_destination(&pos, &link, 4, [0.0, 0.0, 0.0], 1000.0),
            None
        );
        // Empty graph or bad input: no destination.
        assert_eq!(
            pick_road_destination(&pos, &link, 0, [0.0, 0.0, 0.0], 10.0),
            None
        );
        assert_eq!(
            pick_road_destination(&pos, &link, 4, [f32::NAN, 0.0, 0.0], 10.0),
            None
        );
    }

    #[test]
    fn road_path_returns_drive_order_with_graph_distance() {
        // Same chain: path 0-1-2-3 totals 180 m of graph distance.
        let pos = |i: usize| Some([i as f32 * 60.0, 0.0, 0.0]);
        let link = |node: usize, out: &mut Vec<(usize, f32)>| {
            if node > 0 {
                out.push((node - 1, 60.0));
            }
            if node < 3 {
                out.push((node + 1, 60.0));
            }
        };
        assert_eq!(
            road_graph_path(&pos, &link, 4, [5.0, 0.0, 3.0], 3),
            Some((vec![0, 1, 2, 3], 180.0))
        );
        // Start-is-destination: single-element path, zero distance.
        assert_eq!(
            road_graph_path(&pos, &link, 4, [5.0, 0.0, 0.0], 0),
            Some((vec![0], 0.0))
        );
        // Isolated node with a position but no lanes: unreached.
        let pos5 = |i: usize| {
            if i < 4 {
                Some([i as f32 * 60.0, 0.0, 0.0])
            } else {
                Some([1000.0, 0.0, 0.0])
            }
        };
        assert_eq!(road_graph_path(&pos5, &link, 5, [0.0, 0.0, 0.0], 4), None);
        // Invalid destination or bad input: no path.
        assert_eq!(road_graph_path(&pos, &link, 4, [0.0, 0.0, 0.0], 4), None);
        assert_eq!(road_graph_path(&pos, &link, 0, [0.0, 0.0, 0.0], 0), None);
        assert_eq!(
            road_graph_path(&pos, &link, 4, [f32::NAN, 0.0, 0.0], 3),
            None
        );
    }

    #[test]
    fn ground_space_build_covers_mesh_prefix() {
        let tri = [[[0.0, 1.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 1.0]]];
        let space = MeshGroundSpace::build(2, &|i| {
            if i == 0 {
                mesh_aabb(&tri[0])
            } else {
                None
            }
        });
        assert_eq!(space.len(), 2);
        assert!(!space.is_empty());
        assert!(MeshGroundSpace::default().is_empty());
        let tris = |i: usize| {
            if i == 0 {
                Some(tri.to_vec())
            } else {
                None
            }
        };
        assert_eq!(space.ground_below(&tris, 0.25, 0.25, 3.0, 5.0), Some(1.0));
    }

    #[test]
    fn host_dabing_backends_stay_inert_and_record() {
        use crate::dabing::{
            DabingFaceId, DabingFaceSink, DabingOwner, DABING_STATE_FREE, DABING_STATE_SUBTITLE,
        };
        let audio = SilentDabingAudio;
        let mut faces = FaceWeightLog::default();
        assert!(faces.writes().is_empty());
        faces.set_face_weight(DabingFaceId(7), 1, 2, 0.5);
        assert_eq!(faces.writes().len(), 1);
        let (face, region, level, weight) = faces.writes()[0];
        assert_eq!((face, region, level), (DabingFaceId(7), 1, 2));
        assert!((weight - 0.5).abs() < 1e-6);
        // A staged subtitle-only channel fades out through the real tick.
        let mut dub = DabingOwner::new();
        dub.dabing_init();
        dub.begin_subtitle_only(0, 48).unwrap();
        let start_fade = dub.channel(0).unwrap().fade_ms;
        assert!(start_fade > 0);
        assert_eq!(dub.channel(0).unwrap().state, DABING_STATE_SUBTITLE);
        dub.dabing_tick(&audio, &mut faces, start_fade / 2);
        let mid_fade = dub.channel(0).unwrap().fade_ms;
        assert!(mid_fade < start_fade && mid_fade > 0);
        dub.dabing_tick(&audio, &mut faces, mid_fade);
        assert_eq!(dub.channel(0).unwrap().state, DABING_STATE_FREE);
    }

    #[test]
    fn trap_penalty_is_full_at_center_and_zero_outside() {
        let traps = vec![[50.0, 0.0, 10.0]];
        assert_eq!(trap_penalty([50.0, 0.0, 10.0], &traps), TRAP_PENALTY_M);
        assert_eq!(
            trap_penalty([50.0, 100.0, 10.0 + TRAP_RADIUS_M], &traps),
            0.0
        );
        assert_eq!(
            trap_penalty([50.0, 100.0, 10.0 + TRAP_RADIUS_M + 1.0], &traps),
            0.0
        );
        assert_eq!(trap_penalty([0.0, 0.0, 0.0], &[]), 0.0);
    }

    #[test]
    fn trap_penalty_falls_off_linearly_and_stacks() {
        let traps = vec![[0.0, 0.0, 0.0], [0.0, 0.0, 0.0]];
        assert_eq!(
            trap_penalty([TRAP_RADIUS_M / 2.0, 0.0, 0.0], &traps),
            TRAP_PENALTY_M
        );
        let single = vec![[0.0, 0.0, 0.0]];
        assert!(
            (trap_penalty([TRAP_RADIUS_M / 4.0, 0.0, 0.0], &single) - 0.75 * TRAP_PENALTY_M).abs()
                < 1e-3
        );
    }

    #[test]
    fn radius_variants_match_legacy_at_default_radius() {
        // Refactor guard: the _with_radius variants with TRAP_RADIUS_M
        // must equal the legacy entry points bit-for-bit, so existing
        // pins keep covering the default behavior.
        let traps = vec![[10.0, 0.0, 5.0], [-30.0, 0.0, 40.0]];
        for node in [
            [10.0, 0.0, 5.0],
            [10.0 + TRAP_RADIUS_M / 3.0, 0.0, 5.0],
            [10.0 + TRAP_RADIUS_M + 1.0, 0.0, 5.0],
            [0.0, 0.0, 0.0],
        ] {
            assert_eq!(
                trap_penalty_with_radius(node, &traps, TRAP_RADIUS_M),
                trap_penalty(node, &traps)
            );
            assert_eq!(
                trap_edge_penalty_with_radius([0.0, 0.0, 0.0], node, &traps, TRAP_RADIUS_M),
                trap_edge_penalty([0.0, 0.0, 0.0], node, &traps)
            );
        }
    }

    #[test]
    fn narrow_radius_frees_distant_nodes_and_edges() {
        // A node 30 m out pays under the 45 m default but is free at
        // 15 m; the edge term agrees. Pins the ablation dimension.
        let traps = vec![[0.0, 0.0, 0.0]];
        let node = [30.0, 0.0, 0.0];
        assert!(trap_penalty(node, &traps) > 0.0);
        assert_eq!(trap_penalty_with_radius(node, &traps, 15.0), 0.0);
        assert!(trap_edge_penalty([20.0, 0.0, 0.0], node, &traps) > 0.0);
        assert_eq!(
            trap_edge_penalty_with_radius([20.0, 0.0, 0.0], node, &traps, 15.0),
            0.0
        );
        // Non-positive radius penalizes nothing (used as "no traps").
        assert_eq!(trap_penalty_with_radius(node, &traps, 0.0), 0.0);
    }

    #[test]
    fn penalized_dijkstra_routes_around_a_trapped_node() {
        // Diamond: 0 -> {1, 2} -> 3. Node 1 sits on the trap, node 2 is
        // clear; both legs cost the same unpenalized.
        let pos = |i: usize| {
            Some(match i {
                0 => [0.0, 0.0, 0.0],
                1 => [50.0, 0.0, 10.0],
                2 => [50.0, 0.0, -40.0],
                _ => [100.0, 0.0, 0.0],
            })
        };
        fn neighbors(node: usize, out: &mut Vec<(usize, f32)>) {
            match node {
                0 => out.extend([(1, 60.0), (2, 60.0)]),
                1 | 2 => out.push((3, 60.0)),
                _ => {}
            }
        }
        // Sanity: without traps both legs tie; Dijkstra still returns a
        // complete path to the destination.
        let plain = road_graph_path(&pos, &neighbors, 4, [0.0, 0.0, 0.0], 3)
            .expect("plain diamond path exists");
        assert_eq!(plain.0.first(), Some(&0));
        assert_eq!(plain.0.last(), Some(&3));
        assert_eq!(plain.0.len(), 3);
        // With a trap on node 1 the penalized search must avoid it.
        let traps = vec![[50.0, 0.0, 10.0]];
        let penalized = |node: usize, out: &mut Vec<(usize, f32)>| {
            neighbors(node, out);
            if let Some(a) = pos(node) {
                for (other, weight) in out.iter_mut() {
                    if let Some(p) = pos(*other) {
                        *weight += trap_edge_penalty(a, p, &traps);
                    }
                }
            }
        };
        let avoided = road_graph_path(&pos, &penalized, 4, [0.0, 0.0, 0.0], 3)
            .expect("penalized diamond path exists");
        assert_eq!(avoided.0, vec![0, 2, 3]);
        // The 2->3 leg clips the 45 m disc, so it pays a partial segment
        // penalty — but far less than the +2000 of the trapped node.
        assert!(avoided.1 > 120.0 && avoided.1 < 1000.0);
    }

    #[test]
    fn penalized_dijkstra_still_crosses_an_unavoidable_trap() {
        // Single chain 0 -> 1 -> 2 with the trap on the only middle
        // node: the finite penalty keeps the destination reachable
        // instead of failing the search.
        let pos = |i: usize| Some([i as f32 * 50.0, 0.0, 0.0]);
        fn neighbors(node: usize, out: &mut Vec<(usize, f32)>) {
            if node < 2 {
                out.push((node + 1, 50.0));
            }
        }
        let traps = vec![[50.0, 0.0, 0.0]];
        let penalized = |node: usize, out: &mut Vec<(usize, f32)>| {
            neighbors(node, out);
            if let Some(a) = pos(node) {
                for (other, weight) in out.iter_mut() {
                    if let Some(p) = pos(*other) {
                        *weight += trap_edge_penalty(a, p, &traps);
                    }
                }
            }
        };
        let path = road_graph_path(&pos, &penalized, 3, [0.0, 0.0, 0.0], 2)
            .expect("cut-vertex trap stays reachable");
        assert_eq!(path.0, vec![0, 1, 2]);
        assert!(path.1 > 100.0);
    }

    #[test]
    fn trap_edge_penalty_catches_long_cuts_through_the_pocket() {
        // A 300 m straight edge whose endpoints both lie outside the
        // 45 m radius still pays (nearly) full price when its middle
        // crosses the trap; a parallel edge 100 m off pays nothing.
        let traps = vec![[0.0, 0.0, 0.0]];
        let through = trap_edge_penalty([-150.0, 0.0, 0.0], [150.0, 0.0, 0.0], &traps);
        assert!((through - TRAP_PENALTY_M).abs() < 1e-3);
        let around = trap_edge_penalty([-150.0, 0.0, 100.0], [150.0, 0.0, 100.0], &traps);
        assert_eq!(around, 0.0);
        // Degenerate zero-length edges pay the point penalty of their
        // location: nothing far from any trap, full price on top of one.
        assert_eq!(
            trap_edge_penalty([500.0, 0.0, 500.0], [500.0, 0.0, 500.0], &traps),
            0.0
        );
        assert_eq!(
            trap_edge_penalty([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], &traps),
            TRAP_PENALTY_M
        );
    }
}
