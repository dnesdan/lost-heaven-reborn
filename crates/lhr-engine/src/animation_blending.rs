//! Game actor animation blending machine (`C_animations_machine_de_luxe`).
//!
//! Reference: mafia-re pin 113b5b6, `game/actors/C_animations.cpp` and
//! `game/actors/C_animations.h`. Game.exe RVAs are quoted per function as
//! `RVA`/`VA` (`VA = RVA + 0x400000`); the quoted `C_animations.cpp` line
//! numbers are from that pin. No original-runtime validation is claimed here.
//!
//! Scope: the two-slot stage blend plus the slot-2 extended animation and the
//! shared `.tck` track helpers. Full model animation playback, `.tck` file
//! decoding (`C_chunk`), the animation cache owner and per-frame track
//! interpolation stay DEFERRED and surface as explicit
//! [`AnimationUnsupported`] errors at those call sites.
//! `GetTrack` (GAME 0x40e4a0) already lives in `actor_animation` and is not
//! re-claimed here.
#![forbid(unsafe_code)]

/// Version word accepted by `LoadTrack` (`C_animations.h:14`).
pub const TCK_VERSION: i32 = 4;

/// `Tick` result bits (`C_animations.h:18-19`).
pub const ANIMTICK_END: u32 = 1;
/// `Tick` result bit for a missing model (`C_animations.h:19`).
pub const ANIMTICK_NO_MODEL: u32 = 2;

/// Explicit unsupported boundary: file/device/decoder owners live elsewhere.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnimationUnsupported {
    /// Needs the `C_chunk` file backend and the animation-cache owner.
    TrackDeviceMissing,
    /// Needs the `I3D_model` animation slots and the actor/element owners.
    ModelDeviceMissing,
}

impl core::fmt::Display for AnimationUnsupported {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TrackDeviceMissing => write!(f, "track file/device owner missing"),
            Self::ModelDeviceMissing => write!(f, "model animation device missing"),
        }
    }
}

/// Typed projection of `C_animations_machine_de_luxe` blend/extended state.
///
/// Owner pointers are opaque nonzero identities; `None` is the native NULL.
/// Only the words read or written by the ported functions participate here;
/// collision offsets, jump-landing words and PS2-only matrix rows are not
/// modelled.
#[derive(Clone, Debug, PartialEq)]
pub struct AnimationMachine {
    pub actor: Option<u32>,
    pub element: Option<u32>,
    pub model: Option<u32>,
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
    pub track: i32,
    pub prev_track: i32,
    pub next_start: f32,
    pub track_time: u32,
    pub prev_track_time: u32,
    pub prev_blend: f32,
    pub prev_blend_speed: f32,
    pub track_pos: [f32; 3],
    pub prev_track_pos: [f32; 3],
    /// Row-vector 4x4; translation lives in row 3.
    pub track_matrix: [[f32; 4]; 4],
    pub prev_track_matrix: [[f32; 4]; 4],
    pub track_offset: [f32; 3],
    pub correcting: bool,
    pub correction_speed: f32,
    pub correction: f32,
    pub correction_offset: [f32; 3],
    pub ext_on: bool,
    pub ext_fading: bool,
    pub ext_blend: f32,
    pub ext_target: f32,
    pub ext_speed: f32,
    pub ext_once: bool,
    pub ext_end: i32,
    /// Native `unknown_10c`: while set with `ext_on`, the human is not ready.
    pub ext_blocks_ready: bool,
    pub unknown_10e: u16,
    pub disabled: bool,
    pub anim0_id: u32,
    pub anim1_id: u32,
    pub land_height: f32,
}

impl AnimationMachine {
    fn base() -> Self {
        Self {
            actor: None,
            element: None,
            model: None,
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
            track: -1,
            prev_track: -1,
            next_start: -1.0,
            track_time: 0,
            prev_track_time: 0,
            prev_blend: 0.0,
            prev_blend_speed: 0.0,
            track_pos: [0.0; 3],
            prev_track_pos: [0.0; 3],
            track_matrix: identity_matrix(),
            prev_track_matrix: identity_matrix(),
            track_offset: [0.0; 3],
            correcting: false,
            correction_speed: 0.0,
            correction: 0.0,
            correction_offset: [0.0; 3],
            ext_on: false,
            ext_fading: false,
            ext_blend: 0.0,
            ext_target: 0.0,
            ext_speed: 0.0,
            ext_once: false,
            ext_end: 0,
            ext_blocks_ready: false,
            unknown_10e: 0xffff,
            disabled: false,
            anim0_id: u32::MAX,
            anim1_id: u32::MAX,
            land_height: 0.0,
        }
    }
}

fn identity_matrix() -> [[f32; 4]; 4] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// One loaded `.tck` track (`S_track`, `C_animations.h:189`).
///
/// `name` keeps the file name (native `char[32]`); over-long host names are
/// truncated by the constructors below, never grown.
#[derive(Clone, Debug, PartialEq)]
pub struct AnimTrack {
    pub name: String,
    pub length: i32,
    pub key_time: i32,
    pub start_pos: [f32; 3],
    pub end_pos: [f32; 3],
    pub has_directions: bool,
    pub positions: Vec<[f32; 3]>,
    pub directions: Vec<[f32; 3]>,
}

impl AnimTrack {
    pub fn new(
        name: &str,
        length: i32,
        key_time: i32,
        positions: Vec<[f32; 3]>,
        directions: Vec<[f32; 3]>,
    ) -> Self {
        let short: String = name.chars().take(31).collect();
        let has_directions = !directions.is_empty();
        Self {
            name: short,
            length,
            key_time,
            start_pos: [0.0; 3],
            end_pos: [0.0; 3],
            has_directions,
            positions,
            directions,
        }
    }
}

/// FUNCTION: GAME 0x40e510, RVA 0x00e510 (`C_animations.cpp:106-116`,
/// `ClearNoBaseTracks`).
///
/// Drops tracks loaded after the base ones (from the back). With no counted
/// base (`base_track <= 0`) every track goes, matching the `ClearTracks`
/// fallback.
pub fn clear_no_base_tracks(tracks: &mut Vec<AnimTrack>, base_track: i32) {
    if base_track > 0 {
        let keep = (base_track as usize).min(tracks.len());
        tracks.truncate(keep);
    } else {
        tracks.clear();
    }
}

/// FUNCTION: GAME 0x40e660, RVA 0x00e660 (`C_animations.cpp:120-139`,
/// `Init(C_actor *)`).
///
/// Binds the machine to an actor/frame pair and clears the slot/blend/track
/// words exactly as `Reset` does, plus the id words.
pub fn init_actor_machine(actor: Option<u32>, model: Option<u32>) -> AnimationMachine {
    let mut machine = AnimationMachine::base();
    machine.actor = actor;
    machine.element = None;
    machine.model = model;
    machine
}

/// FUNCTION: GAME 0x40e6d0, RVA 0x00e6d0 (`C_animations.cpp:142-161`,
/// `Init(C_traffic_element *)`).
///
/// Same cleared words as the actor init, bound to a traffic element instead.
pub fn init_traffic_machine(element: Option<u32>, model: Option<u32>) -> AnimationMachine {
    let mut machine = AnimationMachine::base();
    machine.actor = None;
    machine.element = element;
    machine.model = model;
    machine
}

/// FUNCTION: GAME 0x40e740, RVA 0x00e740 (`C_animations.cpp:164-183`,
/// `Init(I3D_model *)`).
///
/// Same cleared words, bound directly to a model frame.
pub fn init_model_machine(model: Option<u32>) -> AnimationMachine {
    let mut machine = AnimationMachine::base();
    machine.model = model;
    machine
}

/// FUNCTION: GAME 0x40e7a0, RVA 0x00e7a0 (`C_animations.cpp:186-201`,
/// `Reset`).
///
/// Clears the slot/blend/track/extended words. Owner bindings
/// (`actor`/`element`/`model`) and track payloads are preserved, matching the
/// original which leaves the first three pointers alone.
pub fn reset_machine(machine: &mut AnimationMachine) {
    let base = AnimationMachine::base();
    machine.anim0_on = base.anim0_on;
    machine.anim0_stop_blended = base.anim0_stop_blended;
    machine.anim0_once = base.anim0_once;
    machine.anim1_on = base.anim1_on;
    machine.anim1_stop_blended = base.anim1_stop_blended;
    machine.anim1_once = base.anim1_once;
    machine.stage_blending = base.stage_blending;
    machine.blend = base.blend;
    machine.blend_speed = base.blend_speed;
    machine.anim_index = base.anim_index;
    machine.track = base.track;
    machine.prev_track = base.prev_track;
    machine.next_start = base.next_start;
    machine.ext_on = base.ext_on;
    machine.disabled = base.disabled;
    machine.unknown_10e = base.unknown_10e;
    machine.anim0_id = base.anim0_id;
    machine.anim1_id = base.anim1_id;
}

/// FUNCTION: GAME 0x40e7f0, RVA 0x00e7f0 (`C_animations.cpp:204-218`,
/// `EqualTime`).
///
/// Maps the source slot time into the destination animation length:
/// `dst_end * (src_time / src_len)` with `f32` truncation toward zero.
/// Returns `None` where the original early-outs (missing lengths or
/// `src_time > src_len`); model slot reads stay with the caller.
pub fn equal_time(src_len: u32, src_time: u32, dst_len: u32) -> Option<u32> {
    if src_len == 0 || src_time > src_len {
        return None;
    }
    Some((dst_len as f32 * (src_time as f32 / src_len as f32)) as u32)
}

/// FUNCTION: GAME 0x40e890, RVA 0x00e890 (`C_animations.cpp:221-233`,
/// `GetActiveAnimTime`).
///
/// Reports `(time, length)` of the active slot; `(0, 0)` without a model or
/// without a loaded set on that slot.
pub fn get_active_anim_time(
    model: Option<u32>,
    active_set_end: Option<u32>,
    active_slot_time: u32,
) -> (u32, u32) {
    match (model, active_set_end) {
        (Some(_), Some(length)) => (active_slot_time, length),
        _ => (0, 0),
    }
}

/// Slot-stop sink for `stop_anims` (native `I3D_model::StopAnimation`).
pub trait SlotStopBackend {
    fn stop_animation(&mut self, slot: u32);
}

/// FUNCTION: GAME 0x40e8e0, RVA 0x00e8e0 (`C_animations.cpp:236-243`,
/// `StopAnims`).
///
/// Stops slots 0..8 through the backend. No-op without a model.
pub fn stop_anims(model: Option<u32>, backend: &mut impl SlotStopBackend) {
    if model.is_some() {
        for slot in 0..8 {
            backend.stop_animation(slot);
        }
    }
}

/// FUNCTION: GAME 0x40e900, RVA 0x00e900 (`C_animations.cpp:249-254`,
/// `RecompileMatrix`).
///
/// Moves both stored track origins by `m`, then refreshes the track basis
/// from `local` with its translation row cleared.
pub fn recompile_matrix(machine: &mut AnimationMachine, m: [[f32; 4]; 4], local: [[f32; 4]; 4]) {
    machine.track_pos = transform_point(machine.track_pos, &m);
    machine.prev_track_pos = transform_point(machine.prev_track_pos, &m);
    let mut basis = local;
    basis[3][0] = 0.0;
    basis[3][1] = 0.0;
    basis[3][2] = 0.0;
    machine.track_matrix = basis;
}

fn transform_point(p: [f32; 3], m: &[[f32; 4]; 4]) -> [f32; 3] {
    [
        p[0] * m[0][0] + p[1] * m[1][0] + p[2] * m[2][0] + m[3][0],
        p[0] * m[0][1] + p[1] * m[1][1] + p[2] * m[2][1] + m[3][1],
        p[0] * m[0][2] + p[1] * m[1][2] + p[2] * m[2][2] + m[3][2],
    ]
}

/// FUNCTION: GAME 0x40e950, RVA 0x00e950 (`C_animations.cpp:263-438`,
/// `SetAnim`).
///
/// Explicit unsupported stub: slot selection, track correction blending and
/// the animation-cache fetch need the model/track/cache owners.
pub fn set_anim_stub(_name: &str) -> Result<(), AnimationUnsupported> {
    Err(AnimationUnsupported::ModelDeviceMissing)
}

/// FUNCTION: GAME 0x40e020, RVA 0x00e020 (`C_animations.cpp:45-91`,
/// `LoadTrack`).
///
/// Explicit unsupported stub: the miss path decodes a `.tck` chunk file
/// (`anims\<name>`, version `TCK_VERSION`) through `C_chunk`, which has no
/// owner here. Hits are covered by the pure `actor_animation::get_track`
/// search; this stub never fabricates a track index.
pub fn load_track_stub(_name: &str) -> Result<i32, AnimationUnsupported> {
    Err(AnimationUnsupported::TrackDeviceMissing)
}

/// FUNCTION: GAME 0x40f170, RVA 0x00f170 (`C_animations.cpp:459-462`,
/// `IsActive`).
///
/// True while stage blending runs, a track plays, or the extended animation
/// is active (fading or once-mode), unless disabled.
pub fn is_active(machine: &AnimationMachine) -> bool {
    !machine.disabled
        && (machine.stage_blending
            || machine.track != -1
            || (machine.ext_fading || machine.ext_once) && machine.ext_on)
}

/// FUNCTION: GAME 0x40f1b0, RVA 0x00f1b0 (`C_animations.cpp:494-742`,
/// `Tick`).
///
/// Explicit unsupported stub: per-key track interpolation, actor/element
/// setters, correction spreading and extended-animation fading need the
/// model/actor owners and the key decoder.
pub fn animation_tick_stub(_time: u32) -> Result<u32, AnimationUnsupported> {
    Err(AnimationUnsupported::ModelDeviceMissing)
}

/// FUNCTION: GAME 0x410060, RVA 0x010060 (`C_animations.cpp:745-777`,
/// `SetExtendedAnim`).
///
/// Starts the slot-2 extended animation. `set_end` is the slot-2 set length
/// (`None` = missing set, early return like `!set`). With `speed > 0` the
/// blend fades from 0 to `target`; otherwise it snaps to `target`.
/// Once-mode arms `ext_end` 250 ms before the end (half length for short
/// animations). Returns whether the machine changed.
pub fn set_extended_anim(
    machine: &mut AnimationMachine,
    set_end: Option<u32>,
    target: f32,
    speed: f32,
    once: bool,
) -> bool {
    if machine.disabled || set_end.is_none() {
        return false;
    }
    machine.ext_blocks_ready = false;
    machine.unknown_10e = 0xffff;
    if speed > 0.0 {
        machine.ext_fading = true;
        machine.ext_blend = 0.0;
        machine.ext_target = target;
        machine.ext_speed = speed;
    } else {
        machine.ext_blend = target;
        machine.ext_fading = false;
    }
    machine.ext_once = once;
    machine.ext_on = true;
    if once {
        let length = set_end.unwrap_or(0) as i32;
        let mut end = length - 250;
        if end < 0 {
            end = length / 2;
        }
        machine.ext_end = end;
    }
    true
}

/// FUNCTION: GAME 0x410150, RVA 0x010150 (`C_animations.cpp:780-799`,
/// `DelExtendedAnim`).
///
/// Fades the slot-2 animation out at `speed`, or stops it immediately when
/// `speed <= 0`. No-op while disabled or when no extended animation is on.
/// Returns whether the machine changed.
pub fn del_extended_anim(machine: &mut AnimationMachine, speed: f32) -> bool {
    if machine.disabled || !machine.ext_on {
        return false;
    }
    machine.unknown_10e = 0xffff;
    if speed > 0.0 {
        machine.ext_speed = -speed;
        machine.ext_fading = true;
        machine.ext_target = 0.0;
        machine.ext_once = false;
    } else {
        machine.ext_on = false;
        machine.ext_fading = false;
        machine.ext_once = false;
    }
    true
}

fn find_track<'a>(tracks: &'a [AnimTrack], name: &str) -> Option<&'a AnimTrack> {
    tracks
        .iter()
        .rev()
        .find(|t| t.name.eq_ignore_ascii_case(name))
}

/// FUNCTION: GAME 0x410310, RVA 0x010310 (`C_animations.cpp:825-832`,
/// `GetEndPointOfTCK`).
///
/// Returns the last key position of the named track. Pure lookup: missing
/// tracks report `None` (the original would try `LoadTrack` through the
/// chunk-file device, which has no owner here).
pub fn get_end_point_of_tck(tracks: &[AnimTrack], name: &str) -> Option<[f32; 3]> {
    find_track(tracks, name)?.positions.last().copied()
}

/// FUNCTION: GAME 0x410350, RVA 0x010350 (`C_animations.cpp:836-848`,
/// `GetMixingPointOfTCK`).
///
/// Returns the key 250 ms before the track end
/// (`positions[count - 250 / key_time]`). `None` without the track, for
/// tracks of 250 ms or less, for nonpositive `key_time`, or for a short key
/// list. Pure lookup; file loading stays with the caller.
pub fn get_mixing_point_of_tck(tracks: &[AnimTrack], name: &str) -> Option<[f32; 3]> {
    let track = find_track(tracks, name)?;
    if track.length as u32 <= 250 || track.key_time <= 0 {
        return None;
    }
    let back = 250 / track.key_time;
    if back <= 0 {
        return None;
    }
    let index = track.positions.len().checked_sub(back as usize)?;
    track.positions.get(index).copied()
}

fn rotate_xz(v: [f32; 3], angle: f32) -> [f32; 3] {
    let (s, c) = angle.sin_cos();
    [v[0] * c + v[2] * s, v[1], -v[0] * s + v[2] * c]
}

/// FUNCTION: GAME 0x4101d0, RVA 0x0101d0 (`C_animations.cpp:804-822`,
/// `GetPlacementFromTCK`).
///
/// Displacement from the last key to the first (`first - last`) plus the
/// facing after the turn from the first to the last key direction. The angle
/// is the signed XZ turn `atan2(first) - atan2(last)`; `dir` is `(0,0,1)`
/// rotated by it and the displacement is rotated the same way. `None`
/// without the track or without keys. Pure lookup; file loading stays with
/// the caller.
pub fn get_placement_from_tck(tracks: &[AnimTrack], name: &str) -> Option<([f32; 3], [f32; 3])> {
    let track = find_track(tracks, name)?;
    let first = *track.positions.first()?;
    let last = *track.positions.last()?;
    let mut pos = [first[0] - last[0], first[1] - last[1], first[2] - last[2]];
    if track.has_directions && !track.directions.is_empty() {
        let first_dir = *track.directions.first()?;
        let last_dir = *track.directions.last()?;
        let angle = first_dir[0].atan2(first_dir[2]) - last_dir[0].atan2(last_dir[2]);
        pos = rotate_xz(pos, angle);
        return Some((pos, rotate_xz([0.0, 0.0, 1.0], angle)));
    }
    Some((pos, [0.0, 0.0, 1.0]))
}

/// Stage-blend sink (native `StopAnimation`/`SetAnimBlend`/`Tick`).
pub trait StageBlendBackend {
    fn stop_animation(&mut self, slot: u32);
    fn set_anim_blend(&mut self, slot: u32, blend: f32);
    fn tick_model(&mut self, delta: u32);
}

/// FUNCTION: GAME 0x4103d0, RVA 0x0103d0 (`C_animations.cpp:851-892`,
/// `StageBlendingTick`).
///
/// Advances `blend` by `time_ms * 0.001 * blend_speed`, clamps at `[0, 1]`,
/// stops the blended-out slot (honouring its `stop_blended` word), writes
/// both slot blends and ticks the model with 0. Returns whether the model
/// was ticked; `false` leaves every word untouched.
pub fn stage_blending_tick(
    machine: &mut AnimationMachine,
    time_ms: u32,
    backend: &mut impl StageBlendBackend,
) -> bool {
    if machine.disabled || machine.model.is_none() || !machine.stage_blending {
        return false;
    }
    machine.blend += (time_ms as f32 * 0.001) * machine.blend_speed;
    if machine.blend >= 1.0 {
        machine.blend = 1.0;
        machine.stage_blending = false;
        machine.prev_track = -1;
        if machine.anim1_on {
            backend.stop_animation(1);
            machine.anim1_on = false;
        }
        if machine.anim0_on && machine.anim0_stop_blended {
            backend.stop_animation(0);
            machine.anim0_on = false;
        }
    } else if machine.blend <= 0.0 {
        machine.blend = 0.0;
        machine.stage_blending = false;
        machine.prev_track = -1;
        if machine.anim0_on {
            backend.stop_animation(0);
            machine.anim0_on = false;
        }
        if machine.anim1_on && machine.anim1_stop_blended {
            backend.stop_animation(1);
            machine.anim1_on = false;
        }
    }
    let blend1 = 1.0 - machine.blend;
    backend.set_anim_blend(0, machine.blend);
    backend.set_anim_blend(1, blend1);
    backend.tick_model(0);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tracks() -> Vec<AnimTrack> {
        vec![
            AnimTrack {
                name: "walk.tck".to_string(),
                length: 1000,
                key_time: 100,
                start_pos: [0.0; 3],
                end_pos: [9.0, 0.0, 0.0],
                has_directions: false,
                positions: vec![[0.0, 0.0, 0.0], [9.0, 0.0, 0.0]],
                directions: vec![],
            },
            AnimTrack {
                name: "run.tck".to_string(),
                length: 500,
                key_time: 50,
                start_pos: [0.0; 3],
                end_pos: [4.0, 0.0, 0.0],
                has_directions: true,
                positions: (0..10).map(|i| [i as f32, 0.0, 0.0]).collect(),
                directions: vec![[0.0, 0.0, 1.0]; 10],
            },
        ]
    }

    #[test]
    fn rva_00e020_load_track_stub_needs_device() {
        // RVA 0x00e020: miss path needs the chunk-file device.
        assert_eq!(
            load_track_stub("walk.tck"),
            Err(AnimationUnsupported::TrackDeviceMissing)
        );
    }

    #[test]
    fn rva_00e510_clear_no_base_tracks_truncates() {
        // RVA 0x00e510.
        let mut tracks = sample_tracks();
        clear_no_base_tracks(&mut tracks, 1);
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].name, "walk.tck");
        clear_no_base_tracks(&mut tracks, 0);
        assert!(tracks.is_empty());
    }

    #[test]
    fn rva_00e660_init_actor_machine_clears_words() {
        // RVA 0x00e660.
        let machine = init_actor_machine(Some(7), Some(9));
        assert_eq!((machine.actor, machine.model), (Some(7), Some(9)));
        assert_eq!(machine.element, None);
        assert!(!machine.anim0_on && !machine.stage_blending);
        assert_eq!((machine.track, machine.prev_track), (-1, -1));
        assert_eq!(machine.next_start, -1.0);
        assert_eq!((machine.anim0_id, machine.anim1_id), (u32::MAX, u32::MAX));
    }

    #[test]
    fn rva_00e6d0_init_traffic_machine_binds_element() {
        // RVA 0x00e6d0.
        let machine = init_traffic_machine(Some(3), Some(4));
        assert_eq!((machine.element, machine.model), (Some(3), Some(4)));
        assert_eq!(machine.actor, None);
        assert_eq!(machine.unknown_10e, 0xffff);
    }

    #[test]
    fn rva_00e740_init_model_machine_binds_model_only() {
        // RVA 0x00e740.
        let machine = init_model_machine(Some(11));
        assert_eq!(machine.model, Some(11));
        assert_eq!((machine.actor, machine.element), (None, None));
        assert!(!machine.ext_on && !machine.disabled);
    }

    #[test]
    fn rva_00e7a0_reset_machine_keeps_owners() {
        // RVA 0x00e7a0.
        let mut machine = init_actor_machine(Some(1), Some(2));
        machine.anim0_on = true;
        machine.track = 5;
        machine.anim0_id = 42;
        reset_machine(&mut machine);
        assert_eq!((machine.actor, machine.model), (Some(1), Some(2)));
        assert!(!machine.anim0_on);
        assert_eq!(machine.track, -1);
        assert_eq!(machine.anim0_id, u32::MAX);
    }

    #[test]
    fn rva_00e7f0_equal_time_scales_proportionally() {
        // RVA 0x00e7f0.
        assert_eq!(equal_time(1000, 500, 2000), Some(1000));
        assert_eq!(equal_time(0, 0, 10), None);
        assert_eq!(equal_time(100, 150, 100), None);
    }

    #[test]
    fn rva_00e890_get_active_anim_time_needs_model_and_set() {
        // RVA 0x00e890.
        assert_eq!(get_active_anim_time(Some(1), Some(800), 123), (123, 800));
        assert_eq!(get_active_anim_time(None, Some(800), 123), (0, 0));
        assert_eq!(get_active_anim_time(Some(1), None, 123), (0, 0));
    }

    #[test]
    fn rva_00e8e0_stop_anims_stops_eight_slots() {
        // RVA 0x00e8e0.
        struct Log {
            stops: Vec<u32>,
        }
        impl SlotStopBackend for Log {
            fn stop_animation(&mut self, slot: u32) {
                self.stops.push(slot);
            }
        }
        let mut log = Log { stops: vec![] };
        stop_anims(Some(1), &mut log);
        assert_eq!(log.stops, (0..8).collect::<Vec<_>>());
        stop_anims(None, &mut log);
        assert_eq!(log.stops.len(), 8);
    }

    #[test]
    fn rva_00e900_recompile_matrix_moves_origins_and_clears_basis() {
        // RVA 0x00e900.
        let mut machine = init_model_machine(Some(1));
        machine.track_pos = [1.0, 0.0, 0.0];
        let shift = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [5.0, 0.0, 0.0, 1.0],
        ];
        recompile_matrix(&mut machine, shift, shift);
        assert_eq!(machine.track_pos, [6.0, 0.0, 0.0]);
        assert_eq!(
            (machine.track_matrix[3][0], machine.track_matrix[3][2]),
            (0.0, 0.0)
        );
    }

    #[test]
    fn rva_00e950_set_anim_stub_needs_model_device() {
        // RVA 0x00e950.
        assert_eq!(
            set_anim_stub("walk"),
            Err(AnimationUnsupported::ModelDeviceMissing)
        );
    }

    #[test]
    fn rva_00f170_is_active_matches_native_terms() {
        // RVA 0x00f170.
        let mut machine = init_model_machine(Some(1));
        assert!(!is_active(&machine));
        machine.stage_blending = true;
        assert!(is_active(&machine));
        machine.stage_blending = false;
        machine.track = 2;
        assert!(is_active(&machine));
        machine.track = -1;
        machine.ext_on = true;
        machine.ext_once = true;
        assert!(is_active(&machine));
        machine.disabled = true;
        assert!(!is_active(&machine));
    }

    #[test]
    fn rva_00f1b0_animation_tick_stub_needs_model_device() {
        // RVA 0x00f1b0.
        assert_eq!(
            animation_tick_stub(16),
            Err(AnimationUnsupported::ModelDeviceMissing)
        );
    }

    #[test]
    fn rva_010060_set_extended_anim_fades_and_arms_end() {
        // RVA 0x010060.
        let mut machine = init_model_machine(Some(1));
        assert!(set_extended_anim(&mut machine, Some(1000), 0.7, 2.0, true));
        assert!(machine.ext_on && machine.ext_fading && machine.ext_once);
        assert_eq!((machine.ext_blend, machine.ext_target), (0.0, 0.7));
        assert_eq!(machine.ext_end, 750);
        assert!(!set_extended_anim(&mut machine, None, 1.0, 1.0, false));
        machine.disabled = true;
        assert!(!set_extended_anim(&mut machine, Some(100), 1.0, 1.0, false));
    }

    #[test]
    fn rva_010150_del_extended_anim_fades_or_stops() {
        // RVA 0x010150.
        let mut machine = init_model_machine(Some(1));
        assert!(!del_extended_anim(&mut machine, 1.0));
        set_extended_anim(&mut machine, Some(500), 1.0, 0.0, false);
        assert!(del_extended_anim(&mut machine, 4.0));
        assert_eq!(machine.ext_speed, -4.0);
        assert!(del_extended_anim(&mut machine, 0.0));
        assert!(!machine.ext_on && !machine.ext_fading);
    }

    #[test]
    fn rva_0101d0_get_placement_from_tck_reports_displacement() {
        // RVA 0x0101d0.
        let tracks = sample_tracks();
        let (pos, dir) = get_placement_from_tck(&tracks, "WALK.TCK").unwrap();
        assert_eq!(pos, [-9.0, 0.0, 0.0]);
        assert_eq!(dir, [0.0, 0.0, 1.0]);
        assert!(get_placement_from_tck(&tracks, "missing.tck").is_none());
    }

    #[test]
    fn rva_010310_get_end_point_of_tck_returns_last_key() {
        // RVA 0x010310.
        let tracks = sample_tracks();
        assert_eq!(
            get_end_point_of_tck(&tracks, "run.tck"),
            Some([9.0, 0.0, 0.0])
        );
        assert_eq!(get_end_point_of_tck(&tracks, "missing.tck"), None);
    }

    #[test]
    fn rva_010350_get_mixing_point_of_tck_steps_back_250ms() {
        // RVA 0x010350.
        let tracks = sample_tracks();
        // run: 10 keys, key_time 50 -> index 10 - 5 = 5.
        assert_eq!(
            get_mixing_point_of_tck(&tracks, "run.tck"),
            Some([5.0, 0.0, 0.0])
        );
        let mut short = sample_tracks();
        short[1].length = 200;
        assert_eq!(get_mixing_point_of_tck(&short, "run.tck"), None);
    }

    #[test]
    fn rva_0103d0_stage_blending_tick_clamps_and_ticks() {
        // RVA 0x0103d0.
        struct Log {
            blends: Vec<(u32, f32)>,
            stops: Vec<u32>,
            ticks: Vec<u32>,
        }
        impl StageBlendBackend for Log {
            fn stop_animation(&mut self, slot: u32) {
                self.stops.push(slot);
            }
            fn set_anim_blend(&mut self, slot: u32, blend: f32) {
                self.blends.push((slot, blend));
            }
            fn tick_model(&mut self, delta: u32) {
                self.ticks.push(delta);
            }
        }
        let mut machine = init_model_machine(Some(1));
        machine.stage_blending = true;
        machine.blend = 0.05;
        machine.blend_speed = 100.0;
        machine.anim1_on = true;
        let mut log = Log {
            blends: vec![],
            stops: vec![],
            ticks: vec![],
        };
        assert!(stage_blending_tick(&mut machine, 100, &mut log));
        assert_eq!(machine.blend, 1.0);
        assert!(!machine.stage_blending && !machine.anim1_on);
        assert_eq!(log.ticks, vec![0]);
        assert_eq!(log.blends.len(), 2);
        let mut idle = init_model_machine(Some(1));
        let mut log2 = Log {
            blends: vec![],
            stops: vec![],
            ticks: vec![],
        };
        assert!(!stage_blending_tick(&mut idle, 16, &mut log2));
    }
}
