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
    /// Board the taxi (drivable car leg, see `--car-driving`).
    BoardTaxi,
    /// On-foot leg done; the driving leg stays with the car world.
    OnFootComplete,
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

    /// The player boarded the taxi; the driving leg continues in the car
    /// world (`--car-driving` on the MISE01 map).
    pub fn boarded_taxi(&mut self) {
        if self.stage == Mise01Stage::BoardTaxi {
            self.stage = Mise01Stage::OnFootComplete;
        }
    }

    /// Short HUD line for the current stage.
    pub fn hud_line(&self) -> &'static str {
        match self.stage {
            Mise01Stage::WatchIntro => "MISE01: press C for the intro cutscene, Enter to skip it",
            Mise01Stage::ReachTaxiStand => "MISE01: walk (WASD, Shift runs) to the taxi stand",
            Mise01Stage::BoardTaxi => "MISE01: press E to board the taxi",
            Mise01Stage::OnFootComplete => {
                "MISE01 on foot done: drive the taxi (see --car-driving)"
            }
        }
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
        assert_eq!(progress.stage(), Mise01Stage::OnFootComplete);
    }

    #[test]
    fn walk_input_clamps_and_detects_activity() {
        let held = input(5.0, -5.0, true).clamped();
        assert_eq!((held.forward, held.strafe), (1.0, -1.0));
        assert!(held.is_active());
        assert!(!input(0.0, 0.0, false).is_active());
    }
}
