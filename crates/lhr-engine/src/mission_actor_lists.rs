//! Small C_mission actor/animg-model list operations and C_game startup/tick
//! slices, ported from mafia-re pin 113b5b6 (`game/mission/Mission.cpp` and
//! `game/mission/game.cpp`).
//!
//! Game.exe SHA-256
//! `08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27`;
//! RVAs below are Game base-relative (`VA - 0x400000`). Native actor release
//! counting, frame-detach plumbing (`GetFrameActor`/`SetFrameActor`), device,
//! scene and program owners stay with their callers and are DEFERRED with
//! explicit markers; list order and traversal direction are preserved exactly.
#![forbid(unsafe_code)]

/// Opaque actor handle. Native `C_actor*` identity is the caller's concern.
pub type ActorId = u32;
/// Opaque model handle. Native `I3D_model*` identity is the caller's concern.
pub type ModelId = u32;
/// Opaque frame handle. Native `I3D_frame*` identity is the caller's concern.
pub type FrameId = u32;

/// Entry of `C_mission::anim_models` (mafia-re `game/mission/Mission.h`).
/// Only the compared `model` member and the `game_model` flag participate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnimModel {
    pub model: ModelId,
    pub game_model: bool,
}

/// FUNCTION: GAME RVA 0x13efb0, VA 0x53efb0 (`Mission.cpp:66-69`,
/// `operator<(const C_str&, const C_str&)`).
/// Unsigned-byte `strcmp` ordering: true when `left` sorts before `right`.
/// Slice ends stand in for the NUL terminator of the native `C_str` buffer.
pub fn c_str_less(left: &[u8], right: &[u8]) -> bool {
    let mut index = 0;
    loop {
        let a = if index < left.len() { left[index] } else { 0 };
        let b = if index < right.len() { right[index] } else { 0 };
        if a != b {
            return a < b;
        }
        if a == 0 {
            return false;
        }
        index += 1;
    }
}

/// FUNCTION: GAME RVA 0x13f520, VA 0x53f520 (`Mission.cpp:161-174`,
/// `C_mission::AddAnimModel`).
/// Searched from the last entry; a duplicate model or a model without an
/// animation set is refused. Returns whether the entry was pushed.
pub fn add_anim_model(
    models: &mut Vec<AnimModel>,
    model: ModelId,
    has_animation_set: bool,
) -> bool {
    for index in (0..models.len()).rev() {
        if models[index].model == model {
            return false;
        }
    }
    if !has_animation_set {
        return false;
    }
    models.push(AnimModel {
        model,
        game_model: false,
    });
    true
}

/// FUNCTION: GAME RVA 0x13f710, VA 0x53f710 (`Mission.cpp:178-188`,
/// `C_mission::DelAnimModel`).
/// Searched from the last entry; the ordered `erase` keeps the survivors'
/// relative order. Returns whether an entry was removed.
pub fn del_anim_model(models: &mut Vec<AnimModel>, model: ModelId) -> bool {
    for index in (0..models.len()).rev() {
        if models[index].model == model {
            models.remove(index);
            return true;
        }
    }
    false
}

/// FUNCTION: GAME RVA 0x13f770, VA 0x53f770 (`Mission.cpp:192-204`,
/// `C_mission::ClearGameAnimModels`).
/// Flagged entries leave by moving the last one into their place
/// (`pop_back` tail), so the survivors' order is not preserved.
pub fn clear_game_anim_models(models: &mut Vec<AnimModel>) {
    let mut index = 0;
    while index < models.len() {
        if models[index].game_model {
            models.swap_remove(index);
        } else {
            index += 1;
        }
    }
}

/// Report of `add_actor`: each list admits the actor at most once.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActorAddReport {
    pub actor_added: bool,
    pub not_me_added: bool,
}

/// FUNCTION: GAME RVA 0x13fea0, VA 0x53fea0 (`Mission.cpp:303-325`,
/// `C_mission::AddActor`).
/// Each list is searched from its last actor; `not_me` is only consulted
/// when `not_me_actor` is set.
pub fn add_actor(
    actors: &mut Vec<ActorId>,
    not_me_actors: &mut Vec<ActorId>,
    actor: ActorId,
    not_me_actor: bool,
) -> ActorAddReport {
    for index in (0..actors.len()).rev() {
        if actors[index] == actor {
            return ActorAddReport {
                actor_added: false,
                not_me_added: false,
            };
        }
    }
    actors.push(actor);
    if !not_me_actor {
        return ActorAddReport {
            actor_added: true,
            not_me_added: false,
        };
    }
    for index in (0..not_me_actors.len()).rev() {
        if not_me_actors[index] == actor {
            return ActorAddReport {
                actor_added: true,
                not_me_added: false,
            };
        }
    }
    not_me_actors.push(actor);
    ActorAddReport {
        actor_added: true,
        not_me_added: true,
    }
}

/// FUNCTION: GAME RVA 0x140080, VA 0x540080 (`Mission.cpp:329-351`,
/// `C_mission::UnloadActor`).
/// Ordered `erase` (one `break` per list) from both lists. Returns whether
/// the actor was a mission member; the `not_me_actors` removal is silent.
pub fn unload_actor(
    actors: &mut Vec<ActorId>,
    not_me_actors: &mut Vec<ActorId>,
    actor: ActorId,
) -> bool {
    let mut found = false;
    for index in (0..actors.len()).rev() {
        if actors[index] == actor {
            found = true;
            actors.remove(index);
            break;
        }
    }
    for index in (0..not_me_actors.len()).rev() {
        if not_me_actors[index] == actor {
            not_me_actors.remove(index);
            break;
        }
    }
    found
}

/// Outcome of `del_actor`. The frame is reported so the caller can run the
/// deferred `GetFrameActor`/`SetFrameActor` detach and the `Release`; neither
/// is performed here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DelActorOutcome {
    Removed { frame: Option<FrameId> },
    NotFound,
}

/// FUNCTION: GAME RVA 0x140130, VA 0x540130 (`Mission.cpp:355-385`,
/// `C_mission::DelActor`).
/// Searched from the last actor; the `not_me_actors` occurrence is erased
/// first (ordered, single `break`). The actor's own `Release` and the frame
/// detach stay with the caller (DEFERRED).
pub fn del_actor(
    actors: &mut Vec<ActorId>,
    not_me_actors: &mut Vec<ActorId>,
    actor: ActorId,
    frame_of: impl Fn(ActorId) -> Option<FrameId>,
) -> DelActorOutcome {
    for index in (0..actors.len()).rev() {
        if actors[index] == actor {
            for other in (0..not_me_actors.len()).rev() {
                if not_me_actors[other] == actor {
                    not_me_actors.remove(other);
                    break;
                }
            }
            let frame = frame_of(actor);
            actors.remove(index);
            return DelActorOutcome::Removed { frame };
        }
    }
    DelActorOutcome::NotFound
}

/// FUNCTION: GAME RVA 0x140240, VA 0x540240 (`Mission.cpp:389-407`,
/// `C_mission::DelActors`).
/// Every actor leaves from the last one; both lists end up clear. Returns
/// the detached frames (only actors that report one) in native traversal
/// order so the caller can run the deferred detach and `Release` per actor.
pub fn del_actors(
    actors: &mut Vec<ActorId>,
    not_me_actors: &mut Vec<ActorId>,
    frame_of: impl Fn(ActorId) -> Option<FrameId>,
) -> Vec<FrameId> {
    let mut frames = Vec::new();
    for index in (0..actors.len()).rev() {
        if let Some(frame) = frame_of(actors[index]) {
            frames.push(frame);
        }
    }
    actors.clear();
    not_me_actors.clear();
    frames
}

/// FUNCTION: GAME RVA 0x140300, VA 0x540300 (`Mission.cpp:411-436`,
/// `C_mission::DelNotMeActors`).
/// Each `not_me_actors` entry (from the last one) is looked up from the
/// last actor and erased ordered on match. Returns the detached frames in
/// native traversal order; the `not_me_actors` list ends up clear while
/// unrelated actors stay. Detach and `Release` stay with the caller.
pub fn del_not_me_actors(
    actors: &mut Vec<ActorId>,
    not_me_actors: &mut Vec<ActorId>,
    frame_of: impl Fn(ActorId) -> Option<FrameId>,
) -> Vec<FrameId> {
    let mut frames = Vec::new();
    for index in (0..not_me_actors.len()).rev() {
        let actor = not_me_actors[index];
        for other in (0..actors.len()).rev() {
            if actors[other] == actor {
                if let Some(frame) = frame_of(actor) {
                    frames.push(frame);
                }
                actors.remove(other);
                break;
            }
        }
    }
    not_me_actors.clear();
    frames
}

/// FUNCTION: GAME RVA 0x140420, VA 0x540420 (`Mission.cpp:440-459`,
/// `C_mission::IsOwnActor`).
/// Each list is searched from its last actor: a member that is also listed
/// in `not_me_actors` is not owned.
pub fn is_own_actor(
    actors: &[ActorId],
    not_me_actors: &[ActorId],
    actor: ActorId,
) -> bool {
    for index in (0..actors.len()).rev() {
        if actors[index] == actor {
            for other in (0..not_me_actors.len()).rev() {
                if not_me_actors[other] == actor {
                    return false;
                }
            }
            return true;
        }
    }
    false
}

/// FUNCTION: GAME RVA 0x140490, VA 0x540490 (`Mission.cpp:463-473`,
/// `C_mission::FindActorByName`).
/// A null name finds nothing; otherwise the last actor whose frame name
/// compares equal (`strcmp`) wins. Frame-name lookup stays with the caller.
pub fn find_actor_by_name<'a>(
    actors: &[ActorId],
    name: Option<&'a str>,
    name_of: impl Fn(ActorId) -> Option<&'a str>,
) -> Option<ActorId> {
    let wanted = name?;
    for index in (0..actors.len()).rev() {
        let actor = actors[index];
        if name_of(actor).is_some_and(|candidate| candidate == wanted) {
            return Some(actor);
        }
    }
    None
}

/// Opaque effects-manager handle. Native `C_effects*` identity is the
/// caller's concern; `None` is the native NULL.
pub type EffectsHandle = u32;

/// FUNCTION: GAME RVA 0x7ae40, VA 0x47ae40 (mafia-re
/// `game/mission/Mission.h:224-228`, `C_mission::GetActors`): reads the
/// mission's actor list. The native body returns the owned vector reference;
/// only list membership participates here.
pub fn get_actors(actors: &[ActorId]) -> &[ActorId] {
    actors
}

/// FUNCTION: GAME RVA 0x7ae60, VA 0x47ae60 (mafia-re
/// `game/mission/Mission.h:234-238`, `C_mission::GetEffects`): reads the
/// staged effects-manager handle. The effects object stays with its owner
/// (DEFERRED); `None` is the native NULL.
pub fn get_effects(effects: Option<EffectsHandle>) -> Option<EffectsHandle> {
    effects
}

/// Opaque game-manager handle. Native `C_game*` identity is the caller's
/// concern; `None` is the native NULL.
pub type GameHandle = u32;

/// Working name (no outline RVA is claimed): reads the staged game-manager
/// handle (`C_mission::GetGame`, mafia-re `game/mission/Mission.h:230-233`
/// at pin 113b5b6). The header only notes the outline copy is "most likely"
/// the getter at 0x47ae50, which does not verify an entry, so no RVA is
/// claimed. The game object stays with its owner (DEFERRED).
pub fn get_game(game: Option<GameHandle>) -> Option<GameHandle> {
    game
}

/// Opaque scene handle. Native `I3D_scene*` identity is the caller's
/// concern; `None` is the native NULL.
pub type SceneHandle = u32;

/// Working name (no outline RVA is claimed): reads the staged scene handle
/// (`C_mission::GetScene`, mafia-re `game/mission/Mission.h:279-281` at pin
/// 113b5b6). The header carries no `FUNCTION` marker for it, so no outline
/// entry exists to claim. The scene stays with its owner (DEFERRED).
pub fn get_scene(scene: Option<SceneHandle>) -> Option<SceneHandle> {
    scene
}

/// FUNCTION: GAME RVA 0x140520, VA 0x540520 (mafia-re
/// `game/mission/Mission.cpp:476-481`, `C_mission::GlobalProgramGameInit`):
/// inits every staged whole-game script, from the last one to the first.
/// The program objects and their `Init` stay with the caller behind `init`.
pub fn global_program_game_init<P>(programs: &mut [P], mut init: impl FnMut(&mut P)) {
    for index in (0..programs.len()).rev() {
        init(&mut programs[index]);
    }
}

/// FUNCTION: GAME RVA 0x140550, VA 0x540550 (mafia-re
/// `game/mission/Mission.cpp:483-488`, `C_mission::GlobalProgramGameDone`):
/// runs `Done` on every staged whole-game script, from the last one to the
/// first. The program objects and their `Done` stay with the caller behind
/// `done`.
pub fn global_program_game_done<P>(programs: &mut [P], mut done: impl FnMut(&mut P)) {
    for index in (0..programs.len()).rev() {
        done(&mut programs[index]);
    }
}

/// Scalar actor-numbering counter behind `C_mission::last_actor_number`
/// (mafia-re `game/mission/Mission.h:119-121` at pin 113b5b6): the last
/// number `C_actor::Init` gave an actor, which `Open`
/// (`Mission.cpp:626`) clears. Only the clear and the read are owned here;
/// the numbering itself stays with the actor owner (DEFERRED).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MissionActorNumberState {
    last_actor_number: i32,
}

impl MissionActorNumberState {
    /// Working name (no outline RVA; inline store, mafia-re
    /// `game/mission/Mission.cpp:626`, `C_mission::Open`): clears the last
    /// actor number, as `Open` does before `Close`. No Game.exe function
    /// entry is claimed.
    pub fn reset_last_actor_number(&mut self) {
        self.last_actor_number = 0;
    }

    /// Working name (no outline RVA; counter read, mafia-re
    /// `game/mission/Mission.h:119-121`): reads the last number an actor
    /// was given. No Game.exe function entry is claimed.
    pub fn last_actor_number(&self) -> i32 {
        self.last_actor_number
    }
}

/// Cutscene flag bits of `C_actor::act_flags` (mafia-re pin 113b5b6,
/// `game/actors/C_actor.h:81-84`).
pub const ACTF_CUTSCENE_PLAYED: i32 = 0x1;
pub const ACTF_CUTSCENE_RECORDED: i32 = 0x2;
pub const ACTF_CUTSCENE_PLAY: i32 = 0x4;
pub const ACTF_CUTSCENE_RECORD: i32 = 0x8;

/// FUNCTION: GAME RVA 0x1c27b0, VA 0x5c27b0 (mafia-re pin 113b5b6,
/// `game/mission/C_cutscene_records.cpp:122-133`,
/// `C_actor::SetCutsceneRecord`).
/// Asks for the recording unless the actor is recorded already, or takes both
/// the request and the recorded mark back. Only the flag word is owned here.
pub fn set_cutscene_record(flags: &mut i32, on: bool) {
    if on {
        if (*flags & ACTF_CUTSCENE_RECORDED) == 0 {
            *flags |= ACTF_CUTSCENE_RECORD;
        }
    } else {
        *flags &= !(ACTF_CUTSCENE_RECORD | ACTF_CUTSCENE_RECORDED);
    }
}

/// FUNCTION: GAME RVA 0x1c27d0, VA 0x5c27d0 (mafia-re pin 113b5b6,
/// `game/mission/C_cutscene_records.cpp:135-146`, `C_actor::SetCutscenePlay`).
/// Asks for the playing unless the actor is played already, or takes both
/// the request and the played mark back. Only the flag word is owned here.
pub fn set_cutscene_play(flags: &mut i32, on: bool) {
    if on {
        if (*flags & ACTF_CUTSCENE_PLAYED) == 0 {
            *flags |= ACTF_CUTSCENE_PLAY;
        }
    } else {
        *flags &= !(ACTF_CUTSCENE_PLAY | ACTF_CUTSCENE_PLAYED);
    }
}

/// FUNCTION: GAME RVA 0x1c27f0, VA 0x5c27f0 (mafia-re pin 113b5b6,
/// `game/mission/C_cutscene_records.cpp:148-155`,
/// `C_actor::UpdateCutsceneFlags`).
/// Turns each asked request into its mark, in record then play order. Only
/// the flag word is owned here.
pub fn update_cutscene_flags(flags: &mut i32) {
    if (*flags & ACTF_CUTSCENE_RECORD) != 0 {
        *flags = *flags & !ACTF_CUTSCENE_RECORD | ACTF_CUTSCENE_RECORDED;
    }
    if (*flags & ACTF_CUTSCENE_PLAY) != 0 {
        *flags = *flags & !ACTF_CUTSCENE_PLAY | ACTF_CUTSCENE_PLAYED;
    }
}

/// Entry of `C_mission::t_transparent` (mafia-re `game/mission/Mission.h`).
/// Only the compared `time`/`fade_time` members and the object identity
/// participate; transparency application and release stay with the caller.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransparentEntry {
    pub object: u32,
    pub time: u32,
    pub fade_time: u32,
}

/// Per-object transparency effect the caller must apply for one tick step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransparentEffect {
    pub object: u32,
    pub alpha: f32,
    pub released: bool,
}

/// Subset of `C_mission::Tick`: GAME RVA 0x1407e0, VA 0x5407e0
/// (`Mission.cpp:568-583`, the `t_transparent` fade loop).
/// Time accumulates with `uint` wrapping; a finished object reports alpha
/// `1.0` with `released` set and leaves by swap-with-back plus `pop_back`,
/// while the survivors keep their relative scan order for this step.
/// A zero `fade_time` finishes immediately, so the division below never
/// divides by zero.
pub fn tick_transparent(
    entries: &mut Vec<TransparentEntry>,
    time: u32,
) -> Vec<TransparentEffect> {
    let mut effects = Vec::new();
    let mut index = 0;
    while index < entries.len() {
        entries[index].time = entries[index].time.wrapping_add(time);
        if entries[index].time >= entries[index].fade_time {
            let object = entries[index].object;
            entries.swap_remove(index);
            effects.push(TransparentEffect {
                object,
                alpha: 1.0,
                released: true,
            });
        } else {
            let entry = entries[index];
            effects.push(TransparentEffect {
                object: entry.object,
                alpha: entry.time as f32 / entry.fade_time as f32,
                released: false,
            });
            index += 1;
        }
    }
    effects
}

/// Subset of `C_mission::Tick`: GAME RVA 0x1407e0, VA 0x5407e0
/// (`Mission.cpp:553-558`, the restore-volume head).
/// Consumes the pending restore when no frame has been skipped yet and
/// reports whether it fired; the device `SetVolume` stays with the sound
/// owner (DEFERRED).
pub fn tick_consume_restore_volume(restore: &mut bool, skip_frames: i32) -> bool {
    if *restore && skip_frames < 1 {
        *restore = false;
        true
    } else {
        false
    }
}

/// Classified act-state word for `change_act_state`.
/// Native values from mafia-re `game/actors/C_actor.h:61-63`:
/// `ACTSTATE_ACTIVE 0`, `ACTSTATE_INACTIVE 1`, `ACTSTATE_NONE 2`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActStateClass {
    Active,
    Inactive,
    None,
    Other(u32),
}

/// Live actor participant for `change_act_state`. Both words are reread
/// after every mutating `ChangeState`, as the original does.
pub trait ActStateParticipant {
    fn act_state(&self) -> ActStateClass;
    fn change_state(&mut self);
}

/// FUNCTION: GAME RVA 0x1a5070, VA 0x5a5070 (`game.cpp:1817-1847`,
/// `C_game::ChangeActState`).
/// Clears the pending flag, then moves every actor whose state left its
/// list's class to the other list (an actor that reached `None` leaves both
/// lists). Removals are swap-with-back plus `pop_back` with no index advance;
/// `ChangeState` side effects stay behind the trait (DEFERRED owner).
pub fn change_act_state<A: ActStateParticipant>(
    changed: &mut bool,
    active: &mut Vec<A>,
    inactive: &mut Vec<A>,
) {
    *changed = false;
    let mut index = 0;
    while index < active.len() {
        if active[index].act_state() != ActStateClass::Active {
            active[index].change_state();
            let keep = active[index].act_state() != ActStateClass::None;
            let actor = active.swap_remove(index);
            if keep {
                inactive.push(actor);
            }
        } else {
            index += 1;
        }
    }
    let mut index = 0;
    while index < inactive.len() {
        if inactive[index].act_state() != ActStateClass::Inactive {
            inactive[index].change_state();
            let keep = inactive[index].act_state() != ActStateClass::None;
            let actor = inactive.swap_remove(index);
            if keep {
                active.push(actor);
            }
        } else {
            index += 1;
        }
    }
}

/// Player-control sink for `update_controls_players`: classification stays
/// with the actor owner, the control refresh with the player owner.
pub trait PlayerControlsBackend<A> {
    fn is_player(&self, actor: &A) -> bool;
    fn update_controls(&mut self, actor: &A);
}

/// FUNCTION: GAME RVA 0x1a7ff0, VA 0x5a7ff0 (`game.cpp:2500-2509`,
/// `C_game::UpdateControls`).
/// While initialized, every player actor (scanned from the last one)
/// refreshes its controls through the backend.
pub fn update_controls_players<A>(
    initialized: bool,
    actors: &[A],
    backend: &mut impl PlayerControlsBackend<A>,
) {
    if !initialized {
        return;
    }
    for index in (0..actors.len()).rev() {
        if backend.is_player(&actors[index]) {
            backend.update_controls(&actors[index]);
        }
    }
}

/// FUNCTION: GAME RVA 0x1a3960, VA 0x5a3960 (`game.cpp:1029-1036`,
/// `_show_progress_`).
/// The whole contract is the null gate: with no loading screen there is
/// nothing to drive. Returns whether the caller must run
/// `SetProgressGameInit(fraction)` followed by `Present()`; the device
/// calls stay with the load-save owner (DEFERRED).
pub fn show_progress_gate(has_progress_screen: bool) -> bool {
    has_progress_screen
}

/// Explicit-unsupported boundary for `PrepareBaseBlocks`:
/// GAME RVA 0x1a3990, VA 0x5a3990 (`game.cpp:1040-1043`).
/// The original only logs the stub marker; the base-block owner that would
/// give this a behavior does not exist in this tree.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameStartupUnsupported {
    PrepareBaseBlocks,
}

/// STUB: GAME RVA 0x1a3990, VA 0x5a3990 (`game.cpp:1040-1043`,
/// `PrepareBaseBlocks`). Always reports the unsupported boundary.
pub fn prepare_base_blocks() -> Result<(), GameStartupUnsupported> {
    Err(GameStartupUnsupported::PrepareBaseBlocks)
}

/// Scalar mission counters behind the `C_mission` inline accessors
/// (`Mission.h:239-266` at pin 113b5b6) plus the render-number latch
/// (`Mission.h:123-124`). The scene, actors, scripts and game
/// objects stay with their owners; only the mission-number pair, the
/// skip-frames count these inlines read or write, and the render-number
/// latch `Tick` copies each frame are owned here.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MissionCounterState {
    next_mission_number: i32,
    mission_number: i32,
    skip_frames: i32,
    render_number: i32,
}

impl MissionCounterState {
    /// FUNCTION: GAME 0x47ae70 (RVA 0x7ae70, VA 0x47ae70,
    /// `Mission.h:241-245`, `C_mission::SetMissionNumber`): stores the next
    /// mission number (`SETMISSIONNUMBER`'s).
    pub fn set_mission_number(&mut self, n: i32) {
        self.next_mission_number = n;
    }

    /// Reads the pending value written by
    /// [`MissionCounterState::set_mission_number`].
    pub fn next_mission_number(&self) -> i32 {
        self.next_mission_number
    }

    /// FUNCTION: GAME 0x47ae80 (RVA 0x7ae80, VA 0x47ae80,
    /// `Mission.h:246-250`, `C_mission::GetMissionNumber`): reads the current
    /// mission number (`GETMISSIONNUMBER`'s).
    pub fn get_mission_number(&self) -> i32 {
        self.mission_number
    }

    /// FUNCTION: GAME 0x47ae90 (RVA 0x7ae90, VA 0x47ae90,
    /// `Mission.h:251-256`, `C_mission::KeepMissionNumber`): the pending
    /// number becomes the current one (`CHANGE_MISSION`'s) and the pending
    /// slot is cleared.
    pub fn keep_mission_number(&mut self) {
        self.mission_number = self.next_mission_number;
        self.next_mission_number = 0;
    }

    /// FUNCTION: GAME 0x47aea0 (RVA 0x7aea0, VA 0x47aea0,
    /// `Mission.h:257-261`, `C_mission::SetSkipFrames`): stores the count of
    /// frames `Tick` runs without presenting.
    pub fn set_skip_frames(&mut self, value: i32) {
        self.skip_frames = value;
    }

    /// FUNCTION: GAME 0x47aeb0 (RVA 0x7aeb0, VA 0x47aeb0,
    /// `Mission.h:262-266`, `C_mission::GetSkipFrames`): reads the count
    /// written by [`MissionCounterState::set_skip_frames`].
    pub fn get_skip_frames(&self) -> i32 {
        self.skip_frames
    }

    /// Working name (no outline RVA; inline, mafia-re
    /// `game/mission/Mission.h:290-294`, `C_mission::ResetMissionNumbers`):
    /// clears both the pending and the current mission number. `main` runs
    /// this for each main-menu choice; the native body is inlined, so no
    /// Game.exe function entry is claimed.
    pub fn reset_mission_numbers(&mut self) {
        self.next_mission_number = 0;
        self.mission_number = 0;
    }

    /// Working name (no outline RVA; latch write, mafia-re
    /// `game/mission/Mission.h:123-124`, `C_mission::render_number` at
    /// +0x64): stages the scene render number `Tick` copies each frame.
    /// The native copy is inline in `Tick` with no dedicated function, so
    /// no Game.exe function entry is claimed.
    pub fn set_render_number(&mut self, value: i32) {
        self.render_number = value;
    }

    /// Working name (no outline RVA; latch read, mafia-re
    /// `game/mission/Mission.h:123-124`): reads the staged render number.
    /// No dedicated native function exists, so no RVA is claimed.
    pub fn get_render_number(&self) -> i32 {
        self.render_number
    }
}

/// Presence flag behind `C_mission::opened` (mafia-re
/// `game/mission/Mission.h:119-122`) plus the restore-volume latch
/// (`Mission.h:149-153`). The scene, actors, scripts and game
/// objects stay with their owners; only the flag `Open` sets and `Close`
/// checks-then-clears (`Mission.cpp:512-513`) and the volume latch `Open`
/// sets and `Tick` consumes are owned here.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MissionPresenceState {
    opened: bool,
    mission_name: Option<String>,
    restore_volume: bool,
}

impl MissionPresenceState {
    /// Working name (no outline RVA; flag read, mafia-re
    /// `game/mission/Mission.h:122`): whether a mission is open. No outline
    /// accessor exists in the reference; `Close` early-outs while this is
    /// clear, so no Game.exe function entry is claimed.
    pub fn is_opened(&self) -> bool {
        self.opened
    }

    /// Working name (no outline RVA; flag write, mafia-re
    /// `game/mission/Mission.h:122`): stages the flag `Open` sets and `Close`
    /// clears. The open/close sequencing itself stays with the mission
    /// owner.
    pub fn set_opened(&mut self, opened: bool) {
        self.opened = opened;
    }

    /// Working name (no outline RVA; name read, mafia-re
    /// `game/mission/Mission.h:275-277`, `C_mission::GetMissionName`): the
    /// open mission's name. The reference marks `FUNCTION: GAME 0x47afb0
    /// FOLDED`: the body is shared with `C_actor::GetFrame` (already ported
    /// at RVA 0x7afb0), so no unique Game.exe function entry is claimed
    /// here. `None` is the native NULL. `Open` (`Mission.cpp:638-639`)
    /// stores a copy of the name, `UnInit` (`Mission.cpp:152-156`) deletes
    /// it; allocation stays with the caller staging the value.
    pub fn mission_name(&self) -> Option<&str> {
        self.mission_name.as_deref()
    }

    /// Staging for [`MissionPresenceState::mission_name`]; the `Open`
    /// copy and the `UnInit` delete stay with the mission owner.
    pub fn stage_mission_name(&mut self, name: Option<String>) {
        self.mission_name = name;
    }

    /// Working name (no outline RVA; latch read, mafia-re
    /// `game/mission/Mission.h:149-153`, `C_mission::restore_volume` at
    /// +0x7c): whether `Tick` still owes the post-load volume restore.
    /// `Open` sets it while muting, `Tick` consumes it once `skip_frames`
    /// drops below 1 (see `tick_consume_restore_volume`); no dedicated
    /// native getter exists, so no RVA is claimed.
    pub fn restore_volume(&self) -> bool {
        self.restore_volume
    }

    /// Working name (no outline RVA; latch write, mafia-re
    /// `game/mission/Mission.h:149-153`): stages the volume-restore latch.
    /// The mute/apply sequencing stays with the sound owner.
    pub fn set_restore_volume(&mut self, restore: bool) {
        self.restore_volume = restore;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_str_less_rva_13efb0_matches_strcmp_ordering() {
        assert!(c_str_less(b"a", b"b"));
        assert!(!c_str_less(b"b", b"a"));
        assert!(!c_str_less(b"same", b"same"));
        assert!(c_str_less(b"prefix", b"prefix-long"));
        assert!(!c_str_less(b"", b""));
        assert!(c_str_less(b"", b"x"));
        // Unsigned bytes: 0xFF sorts after ASCII, unlike a signed-char build.
        assert!(!c_str_less(&[0xff], b"a"));
        assert!(c_str_less(b"mission", b"missions"));
    }

    #[test]
    fn add_anim_model_rva_13f520_dedups_and_gates_on_animation_set() {
        let mut models = Vec::new();
        assert!(add_anim_model(&mut models, 7, true));
        assert!(!add_anim_model(&mut models, 7, true));
        assert!(!add_anim_model(&mut models, 9, false));
        assert_eq!(
            models,
            vec![AnimModel {
                model: 7,
                game_model: false
            }]
        );
    }

    #[test]
    fn del_anim_model_rva_13f710_removes_ordered_and_reports() {
        let mut models = vec![
            AnimModel {
                model: 1,
                game_model: false,
            },
            AnimModel {
                model: 2,
                game_model: true,
            },
            AnimModel {
                model: 3,
                game_model: false,
            },
        ];
        assert!(del_anim_model(&mut models, 2));
        assert_eq!(
            models.iter().map(|entry| entry.model).collect::<Vec<_>>(),
            vec![1, 3]
        );
        assert!(!del_anim_model(&mut models, 2));
        assert!(!del_anim_model(&mut Vec::new(), 1));
    }

    #[test]
    fn clear_game_anim_models_rva_13f770_swap_removes_flagged() {
        let mut models = vec![
            AnimModel {
                model: 1,
                game_model: false,
            },
            AnimModel {
                model: 2,
                game_model: true,
            },
            AnimModel {
                model: 3,
                game_model: true,
            },
            AnimModel {
                model: 4,
                game_model: false,
            },
        ];
        clear_game_anim_models(&mut models);
        let mut kept: Vec<ModelId> = models.iter().map(|entry| entry.model).collect();
        kept.sort_unstable();
        assert_eq!(kept, vec![1, 4]);
        assert!(models.iter().all(|entry| !entry.game_model));
    }

    #[test]
    fn add_actor_rva_13fea0_dedups_both_lists() {
        let (mut actors, mut not_me) = (Vec::new(), Vec::new());
        assert_eq!(
            add_actor(&mut actors, &mut not_me, 5, true),
            ActorAddReport {
                actor_added: true,
                not_me_added: true
            }
        );
        assert_eq!(
            add_actor(&mut actors, &mut not_me, 5, true),
            ActorAddReport {
                actor_added: false,
                not_me_added: false
            }
        );
        assert_eq!(
            add_actor(&mut actors, &mut not_me, 6, false),
            ActorAddReport {
                actor_added: true,
                not_me_added: false
            }
        );
        assert_eq!(actors, vec![5, 6]);
        assert_eq!(not_me, vec![5]);
    }

    #[test]
    fn unload_actor_rva_140080_erases_ordered_and_reports_membership() {
        let (mut actors, mut not_me) = (vec![1, 2, 3], vec![2, 9]);
        assert!(unload_actor(&mut actors, &mut not_me, 2));
        assert_eq!(actors, vec![1, 3]);
        assert_eq!(not_me, vec![9]);
        assert!(!unload_actor(&mut actors, &mut not_me, 2));
        assert!(!unload_actor(&mut actors, &mut not_me, 42));
    }

    #[test]
    fn del_actor_rva_140130_reports_frame_and_erases_not_me_first() {
        let (mut actors, mut not_me) = (vec![1, 2, 3], vec![2]);
        let outcome = del_actor(&mut actors, &mut not_me, 2, |actor| {
            (actor == 2).then_some(20)
        });
        assert_eq!(outcome, DelActorOutcome::Removed { frame: Some(20) });
        assert_eq!(actors, vec![1, 3]);
        assert!(not_me.is_empty());
        assert_eq!(
            del_actor(&mut actors, &mut not_me, 9, |_| None),
            DelActorOutcome::NotFound
        );
        // An actor without a frame still leaves the lists; detach is skipped.
        assert_eq!(
            del_actor(&mut actors, &mut not_me, 1, |_| None),
            DelActorOutcome::Removed { frame: None }
        );
        assert_eq!(actors, vec![3]);
    }

    #[test]
    fn del_actors_rva_140240_clears_both_lists_in_reverse_order() {
        let (mut actors, mut not_me) = (vec![1, 2, 3], vec![2]);
        let frames = del_actors(&mut actors, &mut not_me, |actor| {
            (actor != 2).then_some(actor + 100)
        });
        assert_eq!(frames, vec![103, 101]);
        assert!(actors.is_empty());
        assert!(not_me.is_empty());
    }

    #[test]
    fn del_not_me_actors_rva_140300_keeps_unrelated_actors() {
        let (mut actors, mut not_me) = (vec![1, 2, 3, 4], vec![4, 2]);
        let frames = del_not_me_actors(&mut actors, &mut not_me, |actor| Some(actor + 100));
        assert_eq!(frames, vec![102, 104]);
        assert_eq!(actors, vec![1, 3]);
        assert!(not_me.is_empty());
    }

    #[test]
    fn is_own_actor_rva_140420_excludes_not_me_members() {
        let (actors, not_me) = (vec![1, 2], vec![2]);
        assert!(is_own_actor(&actors, &not_me, 1));
        assert!(!is_own_actor(&actors, &not_me, 2));
        assert!(!is_own_actor(&actors, &not_me, 9));
        assert!(!is_own_actor(&[], &[], 1));
    }

    #[test]
    fn find_actor_by_name_rva_140490_prefers_last_match_and_null() {
        let actors = vec![1, 2, 3];
        let name_of = |actor: ActorId| match actor {
            1 => Some("alpha"),
            2 => Some("beta"),
            3 => Some("beta"),
            _ => None,
        };
        assert_eq!(find_actor_by_name(&actors, Some("beta"), name_of), Some(3));
        assert_eq!(find_actor_by_name(&actors, Some("alpha"), name_of), Some(1));
        assert_eq!(find_actor_by_name(&actors, Some("gamma"), name_of), None);
        assert_eq!(find_actor_by_name(&actors, None, name_of), None);
        assert_eq!(find_actor_by_name(&[], Some("beta"), name_of), None);
    }

    #[test]
    fn tick_transparent_rva_1407e0_fades_and_swap_removes_finished() {
        let mut entries = vec![
            TransparentEntry {
                object: 1,
                time: 0,
                fade_time: 10,
            },
            TransparentEntry {
                object: 2,
                time: 9,
                fade_time: 10,
            },
        ];
        let effects = tick_transparent(&mut entries, 1);
        assert_eq!(
            effects,
            vec![
                TransparentEffect {
                    object: 1,
                    alpha: 0.1,
                    released: false
                },
                TransparentEffect {
                    object: 2,
                    alpha: 1.0,
                    released: true
                },
            ]
        );
        assert_eq!(
            entries,
            vec![TransparentEntry {
                object: 1,
                time: 1,
                fade_time: 10
            }]
        );
        // A zero fade finishes immediately without dividing.
        let mut zero = vec![TransparentEntry {
            object: 3,
            time: 0,
            fade_time: 0,
        }];
        let effects = tick_transparent(&mut zero, 0);
        assert_eq!(
            effects,
            vec![TransparentEffect {
                object: 3,
                alpha: 1.0,
                released: true
            }]
        );
        assert!(zero.is_empty());
    }

    #[test]
    fn tick_consume_restore_volume_rva_1407e0_fires_once_before_first_frame() {
        let mut restore = true;
        assert!(tick_consume_restore_volume(&mut restore, 0));
        assert!(!restore);
        assert!(!tick_consume_restore_volume(&mut restore, 0));
        let mut skipped = true;
        assert!(!tick_consume_restore_volume(&mut skipped, 1));
        assert!(skipped);
    }

    struct ScriptedActor {
        state: ActStateClass,
        after_change: ActStateClass,
        changes: usize,
    }

    impl ActStateParticipant for ScriptedActor {
        fn act_state(&self) -> ActStateClass {
            self.state
        }
        fn change_state(&mut self) {
            self.changes += 1;
            self.state = self.after_change;
        }
    }

    #[test]
    fn change_act_state_rva_1a5070_moves_stale_actors_and_clears_flag() {
        let mut changed = true;
        let mut active = vec![
            ScriptedActor {
                state: ActStateClass::Active,
                after_change: ActStateClass::Active,
                changes: 0,
            },
            ScriptedActor {
                state: ActStateClass::Inactive,
                after_change: ActStateClass::Inactive,
                changes: 0,
            },
            ScriptedActor {
                state: ActStateClass::Inactive,
                after_change: ActStateClass::None,
                changes: 0,
            },
        ];
        let mut inactive = vec![ScriptedActor {
            state: ActStateClass::Active,
            after_change: ActStateClass::Active,
            changes: 0,
        }];
        change_act_state(&mut changed, &mut active, &mut inactive);
        assert!(!changed);
        // One stayed active, one arrived from the inactive list; the
        // NONE-bound actor left both lists without a push.
        assert_eq!(active.len(), 2);
        assert!(active.iter().all(|actor| actor.state == ActStateClass::Active));
        assert_eq!(inactive.len(), 1);
        assert_eq!(inactive[0].state, ActStateClass::Inactive);
        assert_eq!(inactive[0].changes, 1);
    }

    struct ControlProbe {
        players: Vec<u32>,
        refreshed: Vec<u32>,
    }

    impl PlayerControlsBackend<u32> for ControlProbe {
        fn is_player(&self, actor: &u32) -> bool {
            self.players.contains(actor)
        }
        fn update_controls(&mut self, actor: &u32) {
            self.refreshed.push(*actor);
        }
    }

    #[test]
    fn update_controls_players_rva_1a7ff0_scans_backwards_while_initialized() {
        let actors = vec![1, 2, 3];
        let mut probe = ControlProbe {
            players: vec![1, 3],
            refreshed: Vec::new(),
        };
        update_controls_players(true, &actors, &mut probe);
        assert_eq!(probe.refreshed, vec![3, 1]);
        probe.refreshed.clear();
        update_controls_players(false, &actors, &mut probe);
        assert!(probe.refreshed.is_empty());
    }

    #[test]
    fn show_progress_gate_rva_1a3960_reports_screen_presence() {
        assert!(show_progress_gate(true));
        assert!(!show_progress_gate(false));
    }

    #[test]
    fn prepare_base_blocks_rva_1a3990_is_explicitly_unsupported() {
        assert_eq!(
            prepare_base_blocks(),
            Err(GameStartupUnsupported::PrepareBaseBlocks)
        );
    }

    #[test]
    fn set_cutscene_record_rva_1c27b0_asks_once_and_takes_both_back() {
        // GAME RVA 0x1c27b0, VA 0x5c27b0.
        let mut flags = 0;
        set_cutscene_record(&mut flags, true);
        assert_eq!(flags, ACTF_CUTSCENE_RECORD);
        // An already recorded actor is not asked again.
        flags = ACTF_CUTSCENE_RECORDED;
        set_cutscene_record(&mut flags, true);
        assert_eq!(flags, ACTF_CUTSCENE_RECORDED);
        // Taking back clears both the request and the mark.
        flags = ACTF_CUTSCENE_RECORD | ACTF_CUTSCENE_RECORDED | ACTF_CUTSCENE_PLAY;
        set_cutscene_record(&mut flags, false);
        assert_eq!(flags, ACTF_CUTSCENE_PLAY);
    }

    #[test]
    fn set_cutscene_play_rva_1c27d0_asks_once_and_takes_both_back() {
        // GAME RVA 0x1c27d0, VA 0x5c27d0.
        let mut flags = 0;
        set_cutscene_play(&mut flags, true);
        assert_eq!(flags, ACTF_CUTSCENE_PLAY);
        flags = ACTF_CUTSCENE_PLAYED;
        set_cutscene_play(&mut flags, true);
        assert_eq!(flags, ACTF_CUTSCENE_PLAYED);
        flags = ACTF_CUTSCENE_PLAY | ACTF_CUTSCENE_PLAYED | ACTF_CUTSCENE_RECORD;
        set_cutscene_play(&mut flags, false);
        assert_eq!(flags, ACTF_CUTSCENE_RECORD);
    }

    #[test]
    fn update_cutscene_flags_rva_1c27f0_turns_requests_into_marks() {
        // GAME RVA 0x1c27f0, VA 0x5c27f0.
        let mut flags = ACTF_CUTSCENE_RECORD | ACTF_CUTSCENE_PLAY;
        update_cutscene_flags(&mut flags);
        assert_eq!(flags, ACTF_CUTSCENE_RECORDED | ACTF_CUTSCENE_PLAYED);
        // Marks without requests are left alone.
        update_cutscene_flags(&mut flags);
        assert_eq!(flags, ACTF_CUTSCENE_RECORDED | ACTF_CUTSCENE_PLAYED);
        let mut idle = 0;
        update_cutscene_flags(&mut idle);
        assert_eq!(idle, 0);
    }

    #[test]
    fn set_mission_number_rva_7ae70_stores_pending_number() {
        // C_mission::SetMissionNumber GAME RVA 0x7ae70 (VA 0x47ae70).
        let mut state = MissionCounterState::default();
        assert_eq!(state.next_mission_number(), 0);
        state.set_mission_number(4);
        assert_eq!(state.next_mission_number(), 4);
        // The current number is untouched until kept.
        assert_eq!(state.get_mission_number(), 0);
        state.set_mission_number(-1);
        assert_eq!(state.next_mission_number(), -1);
    }

    #[test]
    fn get_mission_number_rva_7ae80_reads_current_number() {
        // C_mission::GetMissionNumber GAME RVA 0x7ae80 (VA 0x47ae80).
        let mut state = MissionCounterState::default();
        assert_eq!(state.get_mission_number(), 0);
        state.set_mission_number(6);
        state.keep_mission_number();
        assert_eq!(state.get_mission_number(), 6);
    }

    #[test]
    fn keep_mission_number_rva_7ae90_moves_pending_and_clears() {
        // C_mission::KeepMissionNumber GAME RVA 0x7ae90 (VA 0x47ae90).
        let mut state = MissionCounterState::default();
        state.set_mission_number(9);
        state.keep_mission_number();
        assert_eq!(state.get_mission_number(), 9);
        assert_eq!(state.next_mission_number(), 0);
        // Keeping an empty pending slot clears the current number.
        state.keep_mission_number();
        assert_eq!(state.get_mission_number(), 0);
        assert_eq!(state.next_mission_number(), 0);
    }

    #[test]
    fn set_skip_frames_rva_7aea0_stores_count() {
        // C_mission::SetSkipFrames GAME RVA 0x7aea0 (VA 0x47aea0).
        let mut state = MissionCounterState::default();
        assert_eq!(state.get_skip_frames(), 0);
        state.set_skip_frames(10);
        assert_eq!(state.get_skip_frames(), 10);
        state.set_skip_frames(i32::MIN);
        assert_eq!(state.get_skip_frames(), i32::MIN);
    }

    #[test]
    fn get_skip_frames_rva_7aeb0_reads_stored_count() {
        // C_mission::GetSkipFrames GAME RVA 0x7aeb0 (VA 0x47aeb0).
        let mut state = MissionCounterState::default();
        state.set_skip_frames(2);
        assert_eq!(state.get_skip_frames(), 2);
        state.set_skip_frames(0);
        assert_eq!(state.get_skip_frames(), 0);
    }

    #[test]
    fn get_actors_rva_7ae40_reads_actor_list() {
        // C_mission::GetActors GAME RVA 0x7ae40 (VA 0x47ae40).
        let actors = vec![1, 2, 3];
        assert_eq!(get_actors(&actors), &[1, 2, 3]);
        let empty: Vec<ActorId> = Vec::new();
        assert!(get_actors(&empty).is_empty());
    }

    #[test]
    fn get_effects_rva_7ae60_reads_staged_handle() {
        // C_mission::GetEffects GAME RVA 0x7ae60 (VA 0x47ae60).
        assert_eq!(get_effects(Some(9)), Some(9));
        assert_eq!(get_effects(None), None);
    }

    #[test]
    fn reset_mission_numbers_working_name_clears_both_numbers() {
        // Inline C_mission::ResetMissionNumbers (Mission.h:290-294); no
        // outline RVA is claimed.
        let mut state = MissionCounterState::default();
        state.set_mission_number(4);
        state.keep_mission_number();
        assert_eq!(state.get_mission_number(), 4);
        state.reset_mission_numbers();
        assert_eq!(state.get_mission_number(), 0);
        assert_eq!(state.next_mission_number(), 0);
    }

    #[test]
    fn presence_working_name_tracks_open_flag() {
        // C_mission::opened flag (Mission.h:122); no outline accessor.
        let mut presence = MissionPresenceState::default();
        assert!(!presence.is_opened());
        presence.set_opened(true);
        assert!(presence.is_opened());
        presence.set_opened(false);
        assert!(!presence.is_opened());
    }

    #[test]
    fn presence_working_name_stages_mission_name() {
        // C_mission::GetMissionName (Mission.h:275-277) is FOLDED with
        // C_actor::GetFrame at RVA 0x7afb0; no unique entry is claimed.
        let mut presence = MissionPresenceState::default();
        assert_eq!(presence.mission_name(), None);
        presence.stage_mission_name(Some("mise01".to_owned()));
        assert_eq!(presence.mission_name(), Some("mise01"));
        presence.stage_mission_name(None);
        assert_eq!(presence.mission_name(), None);
    }

    #[test]
    fn global_program_game_init_rva_140520_inits_from_last() {
        // C_mission::GlobalProgramGameInit GAME RVA 0x140520 (VA 0x540520).
        let mut programs = vec![1u32, 2, 3];
        let mut order = Vec::new();
        global_program_game_init(&mut programs, |program| order.push(*program));
        assert_eq!(order, vec![3, 2, 1]);
        let mut empty: Vec<u32> = Vec::new();
        global_program_game_init(&mut empty, |_| panic!("no program to init"));
    }

    #[test]
    fn global_program_game_done_rva_140550_dones_from_last() {
        // C_mission::GlobalProgramGameDone GAME RVA 0x140550 (VA 0x540550).
        let mut programs = vec![7u32, 9];
        let mut order = Vec::new();
        global_program_game_done(&mut programs, |program| order.push(*program));
        assert_eq!(order, vec![9, 7]);
        let mut empty: Vec<u32> = Vec::new();
        global_program_game_done(&mut empty, |_| panic!("no program to done"));
    }

    #[test]
    fn game_and_scene_working_name_round_trip_handles() {
        // C_mission::GetGame (Mission.h:230-233, outline copy only "most
        // likely" at 0x47ae50) and C_mission::GetScene (Mission.h:279-281,
        // no FUNCTION marker): no outline RVA is claimed for either.
        assert_eq!(get_game(Some(11)), Some(11));
        assert_eq!(get_game(None), None);
        assert_eq!(get_scene(Some(13)), Some(13));
        assert_eq!(get_scene(None), None);
    }

    #[test]
    fn actor_number_working_name_reset_and_read() {
        // C_mission::last_actor_number (Mission.h:119-121), cleared by Open
        // (Mission.cpp:626): no outline RVA is claimed.
        let mut state = MissionActorNumberState::default();
        assert_eq!(state.last_actor_number(), 0);
        state.reset_last_actor_number();
        assert_eq!(state.last_actor_number(), 0);
    }

    #[test]
    fn render_number_working_name_stages_tick_latch() {
        // C_mission::render_number (Mission.h:123-124, +0x64); Tick copies
        // the scene number inline, no outline RVA is claimed.
        let mut state = MissionCounterState::default();
        assert_eq!(state.get_render_number(), 0);
        state.set_render_number(44);
        assert_eq!(state.get_render_number(), 44);
        state.set_render_number(-1);
        assert_eq!(state.get_render_number(), -1);
        // Other counters are untouched.
        assert_eq!(state.get_mission_number(), 0);
        assert_eq!(state.get_skip_frames(), 0);
    }

    #[test]
    fn restore_volume_working_name_stages_latch() {
        // C_mission::restore_volume (Mission.h:149-153, +0x7c); Open sets,
        // Tick consumes, no outline RVA is claimed.
        let mut presence = MissionPresenceState::default();
        assert!(!presence.restore_volume());
        presence.set_restore_volume(true);
        assert!(presence.restore_volume());
        presence.set_restore_volume(false);
        assert!(!presence.restore_volume());
        // Presence flag and name staging are untouched.
        assert!(!presence.is_opened());
        assert_eq!(presence.mission_name(), None);
    }
}
