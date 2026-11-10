//! `C_driver::Tick` leading pure-state prefix (Game.exe chase AI, NOT LS3DF).
//!
//! Game.exe SHA-256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27,
//! base 0x400000. Original entry VA 0x43c4f0 / RVA 0x3c4f0, evidenced name
//! `C_driver::Tick` (mafia-re `game/ai/C_driver.h:73`, `game/ai/C_driver.cpp:16`
//! @ 113b5b6; the mafia-re body is a stub, so no Tick semantics come from it).
//! The state fields below come from the evidenced header instead
//! (`game/ai/C_driver.h:45-68`: `path` vector, `next_node` +0x170, the ms time
//! word `unknown_220` +0x220, `flags` +0x1dc).
//!
//! Native evidence re-observed read-only in the authoring probe session
//! (closed; recorded here, not re-observed in review; no decompiler output
//! is trusted for runtime behavior; the decode below is a read-only capstone
//! projection of observed entry bytes, used only to order the ported effects):
//! REA `analyze_function` 0x43c4f0 parent ev_92c7676ef333da2b6eae2222ed0c273f42851e5f5f1cdb0d10fdbde4a73d8f73
//! with summary ev_378af46a02176159807f7927628b161012345e7f6924a9138eab7deb3d31bec0
//! (`FUN_0043c4f0`, non-thunk, body 0x43c4f0-0x43f043 contiguous 11092 bytes,
//! 2881 instructions / 339 basic blocks / 43 callees / 4 callers / 1 parameter)
//! plus entry `read_bytes` 0x43c4f0+160B
//! ev_7215919d2bc144d2664ec3bb20f553e46df8ae8d96c85ebc4594fdfc66fcaaeb.
//! The full 11KB body is far too large for one prefix; this module ports the
//! largest coherent leading slice 0x43c4f0-0x43c571 and halts at the first
//! world effect, the `CALL 0x4cb710` at 0x43c572 (subject+0x70 method, one
//! zero argument). Everything from that call on (frame/physics/collision,
//! the 0x43c577 flag reads at +0x1dc/+0x19c and zero stores at +0x1c4/+0x1bc,
//! and the whole remaining ~10.7KB incl. the 0x43f035 guard-convergence tail)
//! is DEFERRED with explicit markers below and gets no function credit.
//!
//! Ported slice, in native order (thiscall, ECX = driver, stack `time: u32`):
//! 1. `MOV BYTE [subject+0xAC],0` — sole external write of the slice, routed
//!    through [`AiDriverTickBackend`]. The native code dereferences the +0x10
//!    word unconditionally, so a null subject would fault natively; the port
//!    treats a missing subject as an explicit host precondition and skips the
//!    backend call instead of faulting (documented divergence, pinned by test).
//! 2. Path-size snapshot into +0x64: native computes
//!    `([live+8]-[live+4])/0x70` with the 0x92492493 IMUL + SAR 6 + bias
//!    sequence (full signed division by 112 = `sizeof(S_car_node)`), yielding
//!    0 for the null/empty vector. Note the live pointers sit at +0x164/+0x168
//!    (4 bytes past the header's `path` offset 0x160); the port keeps no raw
//!    layout and stores `path.len()` as the dword instead. The store precedes
//!    all three guards, so early exits still publish the snapshot.
//! 3. Guards, each converging (unverified tail) at 0x43f035: snapshot == 0,
//!    then `next_node >= snapshot` (both unsigned compares), then a null
//!    subject re-test (unreachable after step 1 natively unless a callback
//!    nulled it; kept as defensive order fidelity).
//! 4. `time_accum (+0x21C) += time` with x86 wrapping `ADD` semantics. This is
//!    4 bytes before the header's `unknown_220` ms-time word, so the working
//!    name below is deliberately distinct from it.
//!
//! Name-collision note: `lhr_engine::driver_lifecycle::driver_tick` is the
//! LS3DF `I3D_driver::Tick` (RVA 0x15730, wrapping int add) and is NOT this
//! function. The symbol here carries the `ai_` qualifier so the two owners
//! can never be confused; the overlay entry must key on RVA 0x3c4f0.
//!
//! All member names except the evidenced `Tick`, `path` and `next_node` are
//! working names (NOT original). Unknown native fields acquire no invented
//! meanings; unrecovered bytes are simply absent from this owner.
//!
//! Precision policy: all arithmetic is integer (`u32` wrapping add, `usize`
//! length truncated to the native dword); no float, no host f64, no fused ops.
//! The tests in this module are hand-derived vectors authored without cargo
//! (no execution is claimed).

#![forbid(unsafe_code)]

/// `sizeof(S_car_node)` in bytes, evidenced by the native /112 magic divide
/// (IMUL 0x92492493, SAR 6, bias) over the live path-vector word pair.
pub const DRIVER_PATH_NODE_BYTES: usize = 0x70;

/// One path node. Content is unknown (mafia-re `S_car_node` members are
/// guesses except the layout size), so the element stays opaque; the prefix
/// only ever counts elements, never reads them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AiDriverPathNode([u8; DRIVER_PATH_NODE_BYTES]);

impl AiDriverPathNode {
    /// Supply an opaque node without imposing any member interpretation.
    pub const fn from_bytes(bytes: [u8; DRIVER_PATH_NODE_BYTES]) -> Self {
        Self(bytes)
    }

    /// Raw node bytes, e.g. for future differential harnesses.
    pub const fn as_bytes(&self) -> &[u8; DRIVER_PATH_NODE_BYTES] {
        &self.0
    }
}

/// Opaque identity of the driven subject (native +0x10 word; working name —
/// likely the driven car, NOT proven: the constructor binding was not
/// re-observed this session). The word is never a host address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AiDriverSubject {
    word: u32,
}

impl AiDriverSubject {
    /// Interpret a raw +0x10 word: zero is the native null, anything else is
    /// an opaque live identity.
    pub const fn from_word(word: u32) -> Option<Self> {
        if word == 0 {
            None
        } else {
            Some(Self { word })
        }
    }

    /// The opaque native identity word back.
    pub const fn word(self) -> u32 {
        self.word
    }
}

/// Typed driver-tick owner for the ported leading slice. One authoritative
/// owner holds the subject identity, the live path, the +0x64 snapshot, the
/// +0x170 cursor and the +0x21C accumulator; later slices extend this struct,
/// they do not fork it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiDriverTickState {
    /// Native +0x10 subject pointer (working name, see [`AiDriverSubject`]).
    pub subject: Option<AiDriverSubject>,
    /// Native `path` vector (`C_driver.h:51`); live words observed at
    /// +0x164/+0x168, 4 bytes past the header offset.
    pub path: Vec<AiDriverPathNode>,
    /// Native +0x64 path-size snapshot dword (working name, NOT original).
    pub path_len_snapshot: u32,
    /// Native `next_node` +0x170 (`C_driver.h:52`).
    pub next_node: u32,
    /// Native +0x21C wrapping time accumulator (working name, NOT original;
    /// distinct from the header's `unknown_220` ms-time word at +0x220).
    pub time_accum: u32,
}

impl Default for AiDriverTickState {
    fn default() -> Self {
        Self {
            subject: None,
            path: Vec::new(),
            path_len_snapshot: 0,
            next_node: 0,
            time_accum: 0,
        }
    }
}

/// World-effect boundary for the slice. The only external write in
/// 0x43c4f0-0x43c571 is the subject flag clear; the `CALL 0x4cb710` halt and
/// everything after it have no backend here yet (DEFERRED, see
/// [`ai_driver_tick_prefix`]).
pub trait AiDriverTickBackend {
    /// Native `MOV BYTE [subject+0xAC],0` (working name; that byte's meaning
    /// is unknown). Runs first, before the snapshot store.
    fn clear_subject_byte(&mut self);
}

/// DEFERRED world effects at and after the 0x43c572 halt (no backend, no
/// credit): the `CALL 0x4cb710` subject+0x70 method itself (one zero
/// argument), the post-call flag reads (+0x1dc/+0x19c), the zero stores
/// (+0x1c4/+0x1bc), and the remaining ~10.7KB of steering/frame/collision
/// work including the 0x43f035 guard-convergence tail.
pub const AI_DRIVER_TICK_DEFERRED_FROM: u32 = 0x43c572;

// ---------------------------------------------------------------------------
// AI ghost plumbing (marker-only contract scope) plus the C_game::Tick AI
// dispatch stub. Pedestrian C_entity AI Tick itself stays DEFERRED.
// ---------------------------------------------------------------------------

use crate::human_actor::HumanDynCollId;
use crate::script_scheduler::ActorReferenceId;

/// GAME 0x508150 (RVA 0x108150) `C_entity::AI` (mafia-re 113b5b6
/// game/ai/jp_ai_entity.cpp:170-174, header game/ai/jp_ai_entity.h:164):
/// the pedestrian AI tick. CONTRACT CEILING: no AI behavior is owned here;
/// the whole body stays DEFERRED.
pub const ENTITY_AI_RVA: u32 = 0x108150;
/// Pedestrian `C_entity::AI` Tick runs no owned behavior in this contract.
pub const ENTITY_AI_TICK_DEFERRED: bool = true;
/// GAME 0x5074d0 (RVA 0x1074d0) `C_entity::GameInit` (mafia-re 113b5b6
/// game/ai/jp_ai_entity.cpp:120-124): calls `C_human::GameInit` and empties
/// the staged `ghost_colls` via GAME 0x507b50. Body DEFERRED; the emptying
/// leaf is `human_actor::HumanState::empty_ghost_colls`.
pub const ENTITY_GAMEINIT_RVA: u32 = 0x1074d0;
/// GAME 0x531340 (RVA 0x131340) `ai_groups::Tick` (mafia-re 113b5b6
/// game/ai/jp_ai_groups.cpp:56-83), called from `C_game::Tick`
/// (game/mission/game.cpp:2061). Body DEFERRED; dispatched only.
pub const AI_GROUPS_TICK_RVA: u32 = 0x131340;
/// GAME 0x5325a0 (RVA 0x1325a0) `ai_follow_manager::Update` (mafia-re 113b5b6
/// game/ai/jp_ai_groups.cpp:120-335), called from `C_game::Tick`
/// (game/mission/game.cpp:2062). Body DEFERRED; dispatched only.
pub const AI_FOLLOW_UPDATE_RVA: u32 = 0x1325a0;
/// GAME 0x5350b0 (RVA 0x1350b0) `ai_fight_manager::Tick` (mafia-re 113b5b6
/// game/ai/jp_ai_groups.cpp:410-836), called from `C_game::Tick`
/// (game/mission/game.cpp:2063). Body DEFERRED; dispatched only.
pub const AI_FIGHT_TICK_RVA: u32 = 0x1350b0;

/// One `ai_ghost_actor` item of `C_entity::ai_ghost` @0x1268 (mafia-re 113b5b6
/// game/actors/C_human.h:970-973: an actor, its collision, a time and a
/// radius, 0x24-byte vector first per game/ai/jp_ai_entity.h:284).
/// Working names and widths (NOT original): the actor rides the typed owner
/// handle, the collision rides the opaque dynamic-coll identity (never a host
/// pointer), the time word is an opaque `u32` (units unproven) and the radius
/// is the evidenced `f32` sweep radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AiGhostEntry {
    pub actor: Option<ActorReferenceId>,
    pub collision: HumanDynCollId,
    pub time: u32,
    pub radius: f32,
}

impl AiGhostEntry {
    /// Stage one ghost item without running any AI behavior.
    pub const fn from_parts(
        actor: Option<ActorReferenceId>,
        collision: HumanDynCollId,
        time: u32,
        radius: f32,
    ) -> Self {
        Self {
            actor,
            collision,
            time,
            radius,
        }
    }
}

/// Marker-only owner of the `C_entity::ai_ghost` @0x1268 vector (mafia-re
/// 113b5b6 game/ai/jp_ai_entity.h:284, described at
/// game/actors/C_human.h:970-973). Staging only: the pedestrian Tick refresh
/// (GAME 0x508150) stays DEFERRED, and publication into
/// `C_human::ghost_colls` @0x698 rides [`AiGhostStore::ghost_coll_snapshot`]
/// for the GAME 0x52f920 push leaf. Clearing rides
/// [`AiGhostStore::clear_for_game_init`] for the GAME 0x507b50 empty leaf.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AiGhostStore {
    items: Vec<AiGhostEntry>,
}

impl AiGhostStore {
    /// Stage an empty ghost vector (constructor state).
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Staged ghost items in push order.
    pub fn items(&self) -> &[AiGhostEntry] {
        &self.items
    }

    /// Number of staged ghosts.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether no ghost is staged.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// GAME 0x52f920 (RVA 0x12f920) staging leaf: remember one ghost item;
    /// the caller publishes `entry.collision` through
    /// `human_actor::HumanState::push_ghost_coll` into `ghost_colls` @0x698.
    pub fn push(&mut self, entry: AiGhostEntry) {
        self.items.push(entry);
    }

    /// GAME 0x507b50 (RVA 0x107b50) empty leaf for the `GameInit` lifecycle
    /// (called from `C_entity::GameInit`, GAME 0x5074d0): drops the staged
    /// ghosts. The caller also empties `HumanState::ghost_colls` so both
    /// sides of the lifecycle stay empty together.
    pub fn clear_for_game_init(&mut self) {
        self.items.clear();
    }

    /// Collisions to publish into `C_human::ghost_colls` @0x698, in the
    /// staged push order (one entry per ghost item).
    pub fn ghost_coll_snapshot(&self) -> Vec<HumanDynCollId> {
        self.items.iter().map(|item| item.collision).collect()
    }
}

// ---------------------------------------------------------------------------
// C_entity small accessors plus ai_groups/ai_action small state owners.
// The header-inline accessors below survive as out-of-line copies in the
// 0x460a50 block (docs/game-files.md); each entry cites its header
// definition. All arithmetic is integer; flags are plain `u32` bit ops.
// ---------------------------------------------------------------------------

/// Added-state base stripped at most once by `C_entity::GetAIState` (mafia-re
/// game/ai/jp_ai_entity.h:131-134 at pin fe7e746).
pub const ENTITY_STATE_10000: u32 = 10000;
/// Locked-substate marker stripped in a loop by `C_entity::GetAIState`
/// (mafia-re game/ai/jp_ai_entity.h:131-134 at pin fe7e746).
pub const ENTITY_STATE_666: u32 = 666;
/// `C_entity::ai_flags` no-return-from-panic bit (mafia-re
/// game/ai/jp_ai_entity.h:121-129 at pin fe7e746).
pub const ENTITYF_NORETURNFROMPANIC: u32 = 0x1;
/// `C_entity::ai_flags` no-state-switch bit (mafia-re
/// game/ai/jp_ai_entity.h:121-129 at pin fe7e746).
pub const ENTITYF_NOSTATESWITCH: u32 = 0x2;
/// GAME 0x47ba20 (RVA 0x7ba20) `ai_logs::Clear` (mafia-re
/// game/ai/jp_ai_entity.h:112-118 at pin fe7e746).
pub const AI_LOGS_CLEAR_RVA: u32 = 0x7ba20;
/// GAME 0x47ba50 (RVA 0x7ba50) `C_entity::GetVisLogs` (mafia-re
/// game/ai/jp_ai_entity.h:216-220 at pin fe7e746).
pub const ENTITY_GET_VIS_LOGS_RVA: u32 = 0x7ba50;
/// GAME 0x47ba60 (RVA 0x7ba60) `C_entity::GetSensors` (mafia-re
/// game/ai/jp_ai_entity.h:221-225 at pin fe7e746).
pub const ENTITY_GET_SENSORS_RVA: u32 = 0x7ba60;
/// GAME 0x47ba70 (RVA 0x7ba70) `C_entity::GetLogs` (mafia-re
/// game/ai/jp_ai_entity.h:226-230 at pin fe7e746).
pub const ENTITY_GET_LOGS_RVA: u32 = 0x7ba70;
/// GAME 0x47ba80 (RVA 0x7ba80) `C_entity::GetAIState` (mafia-re
/// game/ai/jp_ai_entity.h:231-242 at pin fe7e746).
pub const ENTITY_GET_AI_STATE_RVA: u32 = 0x7ba80;
/// GAME 0x47bac0 (RVA 0x7bac0) `C_entity::ReturnFromPanic` (mafia-re
/// game/ai/jp_ai_entity.h:243-251 at pin fe7e746).
pub const ENTITY_RETURN_FROM_PANIC_RVA: u32 = 0x7bac0;
/// GAME 0x47baf0 (RVA 0x7baf0) `C_entity::SetNoStateSwitch` (mafia-re
/// game/ai/jp_ai_entity.h:252-259 at pin fe7e746).
pub const ENTITY_SET_NO_STATE_SWITCH_RVA: u32 = 0x7baf0;
/// `C_entity::DelUsingObject` entry, GAME RVA 0x7bb20 (VA 0x47bb20; mafia-re
/// game/ai/jp_ai_entity.h:260-265 at pin fe7e746; `FUNCTION: GAME` code in
/// Game.exe, not a data offset and not another binary).
pub const ENTITY_DEL_USING_OBJECT_ENTRY: u32 = 0x47bb20;
/// GAME 0x47b870 (RVA 0x7b870) `ai_groups::AddFreeActor` (mafia-re
/// game/ai/jp_ai_groups.h:90-96 at pin fe7e746).
pub const AI_GROUPS_ADD_FREE_ACTOR_RVA: u32 = 0x7b870;
/// `ai_action_manager::GetBaseJob` entry, GAME RVA 0x127600 (VA 0x527600;
/// mafia-re game/ai/jp_ai_entity.cpp:48-53 at pin fe7e746; `STUB: GAME` code
/// in Game.exe, not a data offset and not another binary).
pub const AI_ACTION_GET_BASE_JOB_ENTRY: u32 = 0x527600;
/// `ai_group::GetCarFactor` entry, GAME RVA 0x130a70 (VA 0x530a70; mafia-re
/// game/ai/jp_ai_groups.cpp:844-849 at pin fe7e746; `STUB: GAME` code in
/// Game.exe, not a data offset and not another binary).
pub const AI_GROUP_GET_CAR_FACTOR_ENTRY: u32 = 0x530a70;

/// Reached-effect boundary for entity/group calls whose owners stay outside
/// this contract. Each `Err` names the original Game.exe entry it stands in
/// for; no destructive effect is replayed automatically.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiEntityError {
    Unsupported { entry: u32 },
}

/// One remembered character of `ai_logs` (mafia-re
/// game/ai/jp_ai_entity.h:87-98 at pin fe7e746): only the two game-time words
/// the fights' `Tick` reads are carried (seen 0x24, shot 0x34); the remaining
/// bytes stay out of this contract. Working names, offsets evidenced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AiCharacterLog {
    pub seen_time: u32,
    pub shot_time: u32,
}

impl AiCharacterLog {
    /// Stage one remembered-character entry without running any AI behavior.
    /// Working-name constructor, NOT original.
    pub const fn from_parts(seen_time: u32, shot_time: u32) -> Self {
        Self {
            seen_time,
            shot_time,
        }
    }
}

/// Typed owner of the `ai_logs::logs` vector (mafia-re
/// game/ai/jp_ai_entity.h:100-119 at pin fe7e746). Staging only: the log
/// refresh itself stays deferred.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AiLogsState {
    entries: Vec<AiCharacterLog>,
}

impl AiLogsState {
    /// Stage an empty log vector (constructor state).
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Stage entries without running any AI behavior. Working-name staging
    /// constructor, NOT original.
    pub fn from_parts(entries: Vec<AiCharacterLog>) -> Self {
        Self { entries }
    }

    /// Staged log entries in push order.
    pub fn entries(&self) -> &[AiCharacterLog] {
        &self.entries
    }

    /// Number of staged log entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no log entry is staged.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// GAME RVA 0x7ba20 (VA 0x47ba20) `ai_logs::Clear` (mafia-re
    /// game/ai/jp_ai_entity.h:112-118 at pin fe7e746; `FUNCTION: GAME` code in
    /// Game.exe, not a data offset and not another binary; the out-of-line
    /// copy lives in the 0x460a50 block): drops the staged log entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

/// Marker for `ai_vis_logs` (mafia-re game/ai/jp_ai_entity.h:62-72 at pin
/// fe7e746): `Init` clears its eight entries and keeps its owner, but the
/// entry layout is unknown, so this carries no behavior. Working name, NOT
/// original.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AiVisLogsMarker;

/// Marker for `ai_sensors` (mafia-re game/ai/jp_ai_entity.h:74-85 at pin
/// fe7e746): same staging-limit rationale as [`AiVisLogsMarker`]. Working
/// name, NOT original.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AiSensorsMarker;

/// Typed owner of the `C_entity` AI slice read by the small accessors
/// (mafia-re game/ai/jp_ai_entity.h:267-283 at pin fe7e746: `vis_logs` @0xb04,
/// `sensors` @0xbb4, `logs` @0xe88, `ai_state` @0xf40, `ai_flags` @0xf50).
/// One authoritative owner; later slices extend this struct, they do not fork
/// it. Member names are working names (NOT original) except where the header
/// evidences them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EntityAiState {
    vis_logs: AiVisLogsMarker,
    sensors: AiSensorsMarker,
    logs: AiLogsState,
    ai_state: u32,
    ai_flags: u32,
}

impl EntityAiState {
    /// Stage the constructor AI slice: empty logs, zero state and flags.
    pub fn new() -> Self {
        Self::default()
    }

    /// GAME RVA 0x7ba50 (VA 0x47ba50) `C_entity::GetVisLogs` (mafia-re
    /// game/ai/jp_ai_entity.h:216-220 at pin fe7e746; `FUNCTION: GAME` code in
    /// Game.exe, not a data offset and not another binary; the out-of-line
    /// copy lives in the 0x460a50 block): returns the staged vis-logs part.
    pub fn get_vis_logs(&self) -> &AiVisLogsMarker {
        &self.vis_logs
    }

    /// GAME RVA 0x7ba60 (VA 0x47ba60) `C_entity::GetSensors` (mafia-re
    /// game/ai/jp_ai_entity.h:221-225 at pin fe7e746; `FUNCTION: GAME` code in
    /// Game.exe, not a data offset and not another binary; the out-of-line
    /// copy lives in the 0x460a50 block): returns the staged sensors part.
    pub fn get_sensors(&self) -> &AiSensorsMarker {
        &self.sensors
    }

    /// GAME RVA 0x7ba70 (VA 0x47ba70) `C_entity::GetLogs` (mafia-re
    /// game/ai/jp_ai_entity.h:226-230 at pin fe7e746; `FUNCTION: GAME` code in
    /// Game.exe, not a data offset and not another binary; the out-of-line
    /// copy lives in the 0x460a50 block): returns the staged logs part.
    pub fn get_logs(&self) -> &AiLogsState {
        &self.logs
    }

    /// Mutable half of [`EntityAiState::get_logs`]: the original returns a
    /// non-const `ai_logs *`, so mutation runs through this accessor. Same
    /// entry (GAME RVA 0x7ba70, VA 0x47ba70).
    pub fn get_logs_mut(&mut self) -> &mut AiLogsState {
        &mut self.logs
    }

    /// GAME RVA 0x7ba80 (VA 0x47ba80) `C_entity::GetAIState` (mafia-re
    /// game/ai/jp_ai_entity.h:231-242 at pin fe7e746; `FUNCTION: GAME` code in
    /// Game.exe, not a data offset and not another binary; the out-of-line
    /// copy lives in the 0x460a50 block): the state without the added
    /// `ENTITY_STATE_*` markers. Subtracts 10000 at most once, then 666 in a
    /// loop (the original counts the loop by division; the loop is exact here
    /// for all `u32` inputs). Pure `u32` arithmetic, no float.
    pub fn get_ai_state(&self) -> u32 {
        let mut state = self.ai_state;
        if state >= ENTITY_STATE_10000 {
            state -= ENTITY_STATE_10000;
        }
        while state >= ENTITY_STATE_666 {
            state -= ENTITY_STATE_666;
        }
        state
    }

    /// Reads the raw `ai_flags` word. Working-name accessor, NOT original;
    /// the original reads the word inline at its call sites.
    pub fn ai_flags(&self) -> u32 {
        self.ai_flags
    }

    /// GAME RVA 0x7bac0 (VA 0x47bac0) `C_entity::ReturnFromPanic` (mafia-re
    /// game/ai/jp_ai_entity.h:243-251 at pin fe7e746; `FUNCTION: GAME` code in
    /// Game.exe, not a data offset and not another binary; the out-of-line
    /// copy lives in the 0x460a50 block): a set parameter clears
    /// `ENTITYF_NORETURNFROMPANIC`, an unset one sets it. Neighbor bits are
    /// preserved.
    pub fn return_from_panic(&mut self, on: bool) {
        if on {
            self.ai_flags &= !ENTITYF_NORETURNFROMPANIC;
        } else {
            self.ai_flags |= ENTITYF_NORETURNFROMPANIC;
        }
    }

    /// GAME RVA 0x7baf0 (VA 0x47baf0) `C_entity::SetNoStateSwitch` (mafia-re
    /// game/ai/jp_ai_entity.h:252-259 at pin fe7e746; `FUNCTION: GAME` code in
    /// Game.exe, not a data offset and not another binary; the out-of-line
    /// copy lives in the 0x460a50 block): a set parameter sets
    /// `ENTITYF_NOSTATESWITCH`, an unset one clears it. Neighbor bits are
    /// preserved.
    pub fn set_no_state_switch(&mut self, on: bool) {
        if on {
            self.ai_flags |= ENTITYF_NOSTATESWITCH;
        } else {
            self.ai_flags &= !ENTITYF_NOSTATESWITCH;
        }
    }
}

/// Original `C_entity::DelUsingObject` (mafia-re
/// game/ai/jp_ai_entity.h:260-265 at pin fe7e746; `FUNCTION: GAME` VA
/// 0x47bb20, RVA 0x7bb20): takes the entity's action-button object from the
/// game's holders. The `using_object`/game owners stay outside this contract,
/// so the boundary is explicit.
pub fn del_using_object_entity() -> Result<(), AiEntityError> {
    Err(AiEntityError::Unsupported {
        entry: ENTITY_DEL_USING_OBJECT_ENTRY,
    })
}

/// Typed owner of the `ai_groups::free_actors` vector (mafia-re
/// game/ai/jp_ai_groups.h:64-73 at pin fe7e746). Staging only: the `Tick`
/// disposal scan stays deferred.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AiFreeActorStore {
    actors: Vec<ActorReferenceId>,
}

impl AiFreeActorStore {
    /// Stage an empty free-actor vector (constructor state).
    pub fn new() -> Self {
        Self {
            actors: Vec::new(),
        }
    }

    /// Staged free actors in push order.
    pub fn actors(&self) -> &[ActorReferenceId] {
        &self.actors
    }

    /// Number of staged free actors.
    pub fn len(&self) -> usize {
        self.actors.len()
    }

    /// Whether no free actor is staged.
    pub fn is_empty(&self) -> bool {
        self.actors.is_empty()
    }

    /// GAME RVA 0x7b870 (VA 0x47b870) `ai_groups::AddFreeActor` (mafia-re
    /// game/ai/jp_ai_groups.h:90-96 at pin fe7e746; `FUNCTION: GAME` code in
    /// Game.exe, not a data offset and not another binary; the out-of-line
    /// copy lives in the 0x460a50 block): remembers one released actor. The
    /// inline copy's `push_back` insert is inlined here in the same order.
    pub fn add_free_actor(&mut self, actor: ActorReferenceId) {
        self.actors.push(actor);
    }
}

/// One `ai_action` job of an entity (mafia-re game/ai/jp_ai_entity.h:31-43 at
/// pin fe7e746): only the `type` and `parent_id` (0xc) words the base-job
/// walk reads are carried. Working names for the struct, offsets evidenced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AiActionEntry {
    pub action_type: i32,
    pub parent_id: i32,
}

impl AiActionEntry {
    /// Stage one job entry without running any AI behavior. Working-name
    /// constructor, NOT original.
    pub const fn new(action_type: i32, parent_id: i32) -> Self {
        Self {
            action_type,
            parent_id,
        }
    }
}

/// Typed owner of the `ai_action_manager::jobs` vector (mafia-re
/// game/ai/jp_ai_entity.h:45-60 at pin fe7e746). The `idles`/`modifiers`
/// vectors stay out of this contract: `GetBaseJob` walks `jobs` from the
/// last, and whether it consults the others is unproven.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AiActionManager {
    jobs: Vec<AiActionEntry>,
}

impl AiActionManager {
    /// Stage an empty job vector (constructor state).
    pub fn new() -> Self {
        Self { jobs: Vec::new() }
    }

    /// Stage jobs without running any AI behavior. Working-name staging
    /// constructor, NOT original.
    pub fn from_parts(jobs: Vec<AiActionEntry>) -> Self {
        Self { jobs }
    }

    /// Number of staged jobs.
    pub fn len(&self) -> usize {
        self.jobs.len()
    }

    /// Whether no job is staged.
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    /// GAME RVA 0x127600 (VA 0x527600) `ai_action_manager::GetBaseJob`
    /// (mafia-re game/ai/jp_ai_entity.cpp:48-53 and
    /// game/ai/jp_ai_entity.h:55-59 at pin fe7e746; `STUB: GAME` code in
    /// Game.exe, not a data offset and not another binary): the kept body only
    /// logs and returns NULL, but the header-evidenced contract is the last
    /// job whose `parent_id` is 0, `None` for none. The port implements that
    /// contract over the staged `jobs` from the last; `idles`/`modifiers`
    /// stay out of scope.
    pub fn get_base_job(&self) -> Option<&AiActionEntry> {
        self.jobs.iter().rev().find(|job| job.parent_id == 0)
    }
}

/// Opaque identity of a `C_car` in an `ai_group` car list. The native word is
/// a pointer; the port never dereferences it. Working name, NOT original.
pub type AiGroupCarId = u64;

/// Typed owner of an `ai_group`'s car list and factor list (mafia-re
/// game/ai/jp_ai_groups.h:50-54 at pin fe7e746: `cars` @0x40 and
/// `car_factors` @0x50). Staging only.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AiGroupCars {
    cars: Vec<AiGroupCarId>,
    factors: Vec<f32>,
}

impl AiGroupCars {
    /// Stage empty car/factor lists (constructor state).
    pub fn new() -> Self {
        Self {
            cars: Vec::new(),
            factors: Vec::new(),
        }
    }

    /// Stage car/factor lists without running any AI behavior. Working-name
    /// staging constructor, NOT original.
    pub fn from_parts(cars: Vec<AiGroupCarId>, factors: Vec<f32>) -> Self {
        Self { cars, factors }
    }

    /// Number of staged cars.
    pub fn len(&self) -> usize {
        self.cars.len()
    }

    /// Whether no car is staged.
    pub fn is_empty(&self) -> bool {
        self.cars.is_empty()
    }

    /// GAME RVA 0x130a70 (VA 0x530a70) `ai_group::GetCarFactor` (mafia-re
    /// game/ai/jp_ai_groups.cpp:844-849 and game/ai/jp_ai_groups.h:58-61 at
    /// pin fe7e746; `STUB: GAME` code in Game.exe, not a data offset and not
    /// another binary): the kept body only logs and returns 1.0, but the
    /// header-evidenced contract is the factor at the car's index in `cars`,
    /// 1.0 without one. The port implements that contract with bit-exact
    /// `f32` copies; no arithmetic, no fused ops. A short factor list falls
    /// back to 1.0 instead of reading past its end.
    pub fn get_car_factor(&self, car: AiGroupCarId) -> f32 {
        match self.cars.iter().position(|&candidate| candidate == car) {
            Some(index) => self.factors.get(index).copied().unwrap_or(1.0),
            None => 1.0,
        }
    }
}

/// Owned dispatch boundary for the `C_game::Tick` AI slice (mafia-re 113b5b6
/// game/mission/game.cpp:2061-2063). Each arm runs exactly one deferred
/// manager body; no AI behavior is owned here.
pub trait AiManagerTickBackend {
    /// `ai_groups::Tick` (GAME 0x531340). Body DEFERRED.
    fn tick_groups(&mut self);
    /// `ai_follow_manager::Update` (GAME 0x5325a0). Body DEFERRED.
    fn update_follow(&mut self);
    /// `ai_fight_manager::Tick` (GAME 0x5350b0). Body DEFERRED.
    fn tick_fight(&mut self);
}

/// `C_game::Tick` AI dispatch stub (mafia-re 113b5b6
/// game/mission/game.cpp:2061-2063), in the original's order: groups, then
/// follow, then fight. Owned dispatch only; every arm body stays DEFERRED
/// behind [`AiManagerTickBackend`] and is never replayed automatically.
pub fn ai_manager_tick_dispatch(backend: &mut impl AiManagerTickBackend) {
    backend.tick_groups();
    backend.update_follow();
    backend.tick_fight();
}

/// `C_driver::Tick` leading prefix, native bytes 0x43c4f0-0x43c571.
///
/// Effect order matches the entry decode: subject-byte clear (via backend,
/// skipped only for the null subject that would fault natively), path-size
/// snapshot store, the three unsigned guards (empty path, cursor past end,
/// null subject), then the wrapping time accumulation. Any guard halts the
/// tick with the snapshot already published and no accumulation. The halt at
/// [`AI_DRIVER_TICK_DEFERRED_FROM`] is a plain return here; the deferred call
/// is never replayed automatically.
pub fn ai_driver_tick_prefix(
    state: &mut AiDriverTickState,
    time: u32,
    backend: &mut impl AiDriverTickBackend,
) {
    if state.subject.is_some() {
        backend.clear_subject_byte();
    }
    // Native dword store of the element count (byte-size/112 for 0x70-byte
    // nodes); path lengths fit u32 in practice, so the `as` cast is exact
    // for all inhabitable states.
    let snapshot = state.path.len() as u32;
    state.path_len_snapshot = snapshot;
    if snapshot == 0 {
        return;
    }
    if state.next_node >= snapshot {
        return;
    }
    if state.subject.is_none() {
        return;
    }
    state.time_accum = state.time_accum.wrapping_add(time);
}

#[cfg(test)]
mod tests {
    use super::*;

    struct LogBackend {
        clears: u32,
    }

    impl AiDriverTickBackend for LogBackend {
        fn clear_subject_byte(&mut self) {
            self.clears += 1;
        }
    }

    fn node() -> AiDriverPathNode {
        AiDriverPathNode::from_bytes([0u8; DRIVER_PATH_NODE_BYTES])
    }

    fn state_with(
        subject_word: u32,
        path_len: usize,
        next_node: u32,
        accum: u32,
    ) -> AiDriverTickState {
        AiDriverTickState {
            subject: AiDriverSubject::from_word(subject_word),
            path: vec![node(); path_len],
            path_len_snapshot: 0xdead_beef,
            next_node,
            time_accum: accum,
        }
    }

    #[test]
    fn empty_path_publishes_zero_snapshot_and_skips_accum() {
        let mut state = state_with(0x77, 0, 0, 7);
        let mut backend = LogBackend { clears: 0 };
        ai_driver_tick_prefix(&mut state, 5, &mut backend);
        assert_eq!(backend.clears, 1, "subject clear runs before the guard");
        assert_eq!(state.path_len_snapshot, 0);
        assert_eq!(state.time_accum, 7, "halted tick must not accumulate");
    }

    #[test]
    fn cursor_past_end_publishes_snapshot_and_skips_accum() {
        for next_node in [2u32, 3, u32::MAX] {
            let mut state = state_with(0x77, 2, next_node, 9);
            let mut backend = LogBackend { clears: 0 };
            ai_driver_tick_prefix(&mut state, 4, &mut backend);
            assert_eq!(backend.clears, 1);
            assert_eq!(state.path_len_snapshot, 2);
            assert_eq!(state.time_accum, 9);
        }
    }

    #[test]
    fn cursor_at_last_node_still_ticks() {
        let mut state = state_with(0x77, 3, 2, 100);
        let mut backend = LogBackend { clears: 0 };
        ai_driver_tick_prefix(&mut state, 23, &mut backend);
        assert_eq!(state.path_len_snapshot, 3);
        assert_eq!(state.time_accum, 123);
    }

    #[test]
    fn null_subject_skips_backend_clear_but_stores_snapshot() {
        // Native would fault on the unconditional [subject+0xAC] dereference;
        // the port records the host-precondition skip and keeps guard order.
        let mut state = state_with(0, 2, 0, 11);
        let mut backend = LogBackend { clears: 0 };
        ai_driver_tick_prefix(&mut state, 6, &mut backend);
        assert_eq!(backend.clears, 0);
        assert_eq!(state.path_len_snapshot, 2);
        assert_eq!(state.time_accum, 11, "null-subject guard halts the tick");
    }

    #[test]
    fn time_accumulation_wraps_like_x86_add() {
        let mut state = state_with(0x77, 1, 0, 0xffff_fff0);
        let mut backend = LogBackend { clears: 0 };
        ai_driver_tick_prefix(&mut state, 0x20, &mut backend);
        assert_eq!(state.time_accum, 0x10);
        ai_driver_tick_prefix(&mut state, 0xffff_fff0, &mut backend);
        assert_eq!(state.time_accum, 0);
        assert_eq!(backend.clears, 2);
    }

    #[test]
    fn snapshot_tracks_path_length_as_native_dword() {
        for len in [1usize, 2, 3, 5, 17] {
            let mut state = state_with(0x77, len, 0, 0);
            let mut backend = LogBackend { clears: 0 };
            ai_driver_tick_prefix(&mut state, 0, &mut backend);
            assert_eq!(state.path_len_snapshot, len as u32);
            assert_eq!(state.time_accum, 0);
        }
    }

    #[test]
    fn subject_word_zero_is_null_and_element_span_is_evidence() {
        assert_eq!(AiDriverSubject::from_word(0), None);
        assert_eq!(
            AiDriverSubject::from_word(1).map(AiDriverSubject::word),
            Some(1)
        );
        assert_eq!(
            std::mem::size_of::<AiDriverPathNode>(),
            DRIVER_PATH_NODE_BYTES,
            "opaque element must stay 112B to match the native /112 divisor"
        );
    }

    struct OrderBackend {
        order: Vec<&'static str>,
    }

    impl AiManagerTickBackend for OrderBackend {
        fn tick_groups(&mut self) {
            self.order.push("groups");
        }
        fn update_follow(&mut self) {
            self.order.push("follow");
        }
        fn tick_fight(&mut self) {
            self.order.push("fight");
        }
    }

    #[test]
    fn ghost_store_push_snapshot_and_game_init_empty_match_lifecycle() {
        // Marker-only contract for C_entity::ai_ghost @0x1268 items
        // (actor + collision + time + radius, mafia-re 113b5b6
        // game/actors/C_human.h:970-973): GAME 0x52f920 pushes stage items
        // whose collisions snapshot into ghost_colls @0x698 in push order;
        // GAME 0x507b50 (from C_entity::GameInit, GAME 0x5074d0) empties.
        use crate::script_scheduler::ActorReferenceId;
        let mut store = AiGhostStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
        assert!(store.ghost_coll_snapshot().is_empty());
        store.push(AiGhostEntry::from_parts(
            Some(ActorReferenceId::from_existing(11)),
            HumanDynCollId::from_raw(7),
            120,
            0.5,
        ));
        store.push(AiGhostEntry::from_parts(
            None,
            HumanDynCollId::from_raw(9),
            240,
            1.25,
        ));
        assert_eq!(store.len(), 2);
        assert_eq!(
            store.items()[0].actor,
            Some(ActorReferenceId::from_existing(11))
        );
        assert_eq!(
            store.ghost_coll_snapshot(),
            vec![
                HumanDynCollId::from_raw(7),
                HumanDynCollId::from_raw(9)
            ]
        );
        store.clear_for_game_init();
        assert!(store.is_empty());
        assert!(store.ghost_coll_snapshot().is_empty());
        assert!(ENTITY_AI_TICK_DEFERRED);
        assert_eq!(ENTITY_AI_RVA, 0x108150);
    }

    #[test]
    fn game_tick_dispatch_runs_groups_follow_fight_in_order() {
        // Owned dispatch stub for C_game::Tick (mafia-re 113b5b6
        // game/mission/game.cpp:2061-2063): groups (GAME 0x531340), follow
        // update (GAME 0x5325a0), fight (GAME 0x5350b0); bodies DEFERRED.
        let mut backend = OrderBackend { order: Vec::new() };
        ai_manager_tick_dispatch(&mut backend);
        assert_eq!(backend.order, vec!["groups", "follow", "fight"]);
        assert_eq!(AI_GROUPS_TICK_RVA, 0x131340);
        assert_eq!(AI_FOLLOW_UPDATE_RVA, 0x1325a0);
        assert_eq!(AI_FIGHT_TICK_RVA, 0x1350b0);
    }

    #[test]
    fn rva_7ba20_ai_logs_clear_empties_staged_logs() {
        // ai_logs::Clear GAME VA 0x47ba20 (RVA 0x7ba20): drops staged entries.
        assert_eq!(AI_LOGS_CLEAR_RVA, 0x7ba20);
        let mut logs = AiLogsState::from_parts(vec![
            AiCharacterLog::from_parts(100, 200),
            AiCharacterLog::from_parts(300, 400),
        ]);
        assert_eq!(logs.len(), 2);
        assert!(!logs.is_empty());
        logs.clear();
        assert!(logs.is_empty());
        assert!(logs.entries().is_empty());
        // Clearing an empty log stays a no-op.
        logs.clear();
        assert!(logs.is_empty());
    }

    #[test]
    fn rva_7ba50_7ba60_7ba70_entity_accessors_return_owned_parts() {
        // C_entity::GetVisLogs/GetSensors/GetLogs GAME VAs
        // 0x47ba50/0x47ba60/0x47ba70 (RVAs 0x7ba50/0x7ba60/0x7ba70): each
        // returns its own staged part.
        assert_eq!(ENTITY_GET_VIS_LOGS_RVA, 0x7ba50);
        assert_eq!(ENTITY_GET_SENSORS_RVA, 0x7ba60);
        assert_eq!(ENTITY_GET_LOGS_RVA, 0x7ba70);
        let mut state = EntityAiState::new();
        assert_eq!(state.get_vis_logs(), &AiVisLogsMarker);
        assert_eq!(state.get_sensors(), &AiSensorsMarker);
        assert!(state.get_logs().is_empty());
        state.get_logs_mut().clear();
        assert_eq!(state.ai_flags(), 0);
    }

    #[test]
    fn rva_7ba80_get_ai_state_strips_added_state() {
        // C_entity::GetAIState GAME VA 0x47ba80 (RVA 0x7ba80): 10000 off at
        // most once, then 666 in a loop.
        assert_eq!(ENTITY_GET_AI_STATE_RVA, 0x7ba80);
        for (raw, expected) in [
            (0u32, 0u32),
            (665, 665),
            (666, 0),
            (667, 1),
            (1998, 0),
            (10000, 0),
            (10001, 1),
            (10666, 0),
            (10667, 1),
            (20000, 10),
            (10000 + 666 * 7 + 5, 5),
        ] {
            let state = EntityAiState {
                ai_state: raw,
                ..Default::default()
            };
            assert_eq!(state.get_ai_state(), expected, "raw {raw}");
        }
    }

    #[test]
    fn rva_7bac0_return_from_panic_toggles_no_return_flag() {
        // C_entity::ReturnFromPanic GAME VA 0x47bac0 (RVA 0x7bac0): an unset
        // parameter sets ENTITYF_NORETURNFROMPANIC, a set one clears it.
        assert_eq!(ENTITY_RETURN_FROM_PANIC_RVA, 0x7bac0);
        let mut state = EntityAiState::new();
        state.return_from_panic(false);
        assert_eq!(state.ai_flags(), ENTITYF_NORETURNFROMPANIC);
        state.return_from_panic(true);
        assert_eq!(state.ai_flags(), 0);
        // Neighbor bits are preserved both ways.
        state = EntityAiState {
            ai_flags: ENTITYF_NOSTATESWITCH,
            ..Default::default()
        };
        state.return_from_panic(false);
        assert_eq!(
            state.ai_flags(),
            ENTITYF_NOSTATESWITCH | ENTITYF_NORETURNFROMPANIC
        );
        state.return_from_panic(true);
        assert_eq!(state.ai_flags(), ENTITYF_NOSTATESWITCH);
    }

    #[test]
    fn rva_7baf0_set_no_state_switch_toggles_flag() {
        // C_entity::SetNoStateSwitch GAME VA 0x47baf0 (RVA 0x7baf0): a set
        // parameter sets ENTITYF_NOSTATESWITCH, an unset one clears it.
        assert_eq!(ENTITY_SET_NO_STATE_SWITCH_RVA, 0x7baf0);
        let mut state = EntityAiState::new();
        state.set_no_state_switch(true);
        assert_eq!(state.ai_flags(), ENTITYF_NOSTATESWITCH);
        state.set_no_state_switch(false);
        assert_eq!(state.ai_flags(), 0);
        state = EntityAiState {
            ai_flags: ENTITYF_NORETURNFROMPANIC,
            ..Default::default()
        };
        state.set_no_state_switch(true);
        assert_eq!(
            state.ai_flags(),
            ENTITYF_NORETURNFROMPANIC | ENTITYF_NOSTATESWITCH
        );
        state.set_no_state_switch(false);
        assert_eq!(state.ai_flags(), ENTITYF_NORETURNFROMPANIC);
    }

    #[test]
    fn rva_7bb20_del_using_object_reports_unsupported() {
        // C_entity::DelUsingObject GAME VA 0x47bb20 (RVA 0x7bb20).
        assert_eq!(ENTITY_DEL_USING_OBJECT_ENTRY, 0x47bb20);
        assert_eq!(
            del_using_object_entity(),
            Err(AiEntityError::Unsupported { entry: 0x47bb20 })
        );
    }

    #[test]
    fn rva_7b870_add_free_actor_pushes_in_order() {
        // ai_groups::AddFreeActor GAME VA 0x47b870 (RVA 0x7b870).
        use crate::script_scheduler::ActorReferenceId;
        assert_eq!(AI_GROUPS_ADD_FREE_ACTOR_RVA, 0x7b870);
        let mut store = AiFreeActorStore::new();
        assert!(store.is_empty());
        store.add_free_actor(ActorReferenceId::from_existing(4));
        store.add_free_actor(ActorReferenceId::from_existing(9));
        assert_eq!(store.len(), 2);
        assert_eq!(
            store.actors(),
            &[
                ActorReferenceId::from_existing(4),
                ActorReferenceId::from_existing(9)
            ]
        );
    }

    #[test]
    fn rva_127600_get_base_job_returns_last_parent_zero_job() {
        // ai_action_manager::GetBaseJob GAME VA 0x527600 (RVA 0x127600).
        assert_eq!(AI_ACTION_GET_BASE_JOB_ENTRY, 0x527600);
        assert_eq!(AiActionManager::new().get_base_job(), None);
        let manager = AiActionManager::from_parts(vec![
            AiActionEntry::new(1, 7),
            AiActionEntry::new(2, 0),
            AiActionEntry::new(3, 0),
            AiActionEntry::new(4, 9),
        ]);
        assert_eq!(manager.get_base_job(), Some(&AiActionEntry::new(3, 0)));
        let parented = AiActionManager::from_parts(vec![
            AiActionEntry::new(1, 5),
            AiActionEntry::new(2, 6),
        ]);
        assert_eq!(parented.get_base_job(), None);
    }

    #[test]
    fn rva_130a70_get_car_factor_returns_index_factor_or_one() {
        // ai_group::GetCarFactor GAME VA 0x530a70 (RVA 0x130a70).
        assert_eq!(AI_GROUP_GET_CAR_FACTOR_ENTRY, 0x530a70);
        let grouped = AiGroupCars::from_parts(vec![11, 22, 33], vec![0.5, 2.0]);
        assert_eq!(grouped.get_car_factor(11), 0.5);
        assert_eq!(grouped.get_car_factor(22), 2.0);
        // A short factor list falls back to 1.0 instead of reading past it.
        assert_eq!(grouped.get_car_factor(33), 1.0);
        // A car without an entry counts 1.0.
        assert_eq!(grouped.get_car_factor(44), 1.0);
        assert_eq!(AiGroupCars::new().get_car_factor(11), 1.0);
    }
}
