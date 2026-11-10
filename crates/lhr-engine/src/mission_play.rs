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
}

impl Mise01Progress {
    /// Fresh tracker waiting for the intro cutscene.
    pub fn fresh() -> Self {
        Self {
            stage: Mise01Stage::WatchIntro,
            taxi_stand: None,
            taxi_radius_m: 3.0,
        }
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

    /// Report the taxi odometer in metres. Reaching `TAXI_DESTINATION_M`
    /// completes the mission. Out-of-order calls never regress the stage.
    pub fn drive_distance(&mut self, odometer_m: f32) {
        if self.stage == Mise01Stage::DrivingToDestination && odometer_m >= TAXI_DESTINATION_M {
            self.stage = Mise01Stage::Complete;
        }
    }

    /// Short HUD line for the current stage.
    pub fn hud_line(&self) -> &'static str {
        match self.stage {
            Mise01Stage::WatchIntro => "MISE01: press C for the intro cutscene, Enter to skip it",
            Mise01Stage::ReachTaxiStand => "MISE01: walk (WASD, Shift runs) to the taxi stand",
            Mise01Stage::BoardTaxi => "MISE01: press E to board the taxi",
            Mise01Stage::DrivingToDestination => {
                "MISE01: drive the taxi 150 m (W/S/A/D, Space brake)"
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
    if !(0.0 <= u && u <= 1.0) {
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
        progress.drive_distance(10.0);
        assert_eq!(progress.stage(), Mise01Stage::DrivingToDestination);
        progress.drive_distance(TAXI_DESTINATION_M);
        assert_eq!(progress.stage(), Mise01Stage::Complete);
        // Completed stages never regress.
        progress.cutscene_done();
        progress.player_moved([0.0, 0.0, 0.0]);
        progress.drive_distance(0.0);
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
}
