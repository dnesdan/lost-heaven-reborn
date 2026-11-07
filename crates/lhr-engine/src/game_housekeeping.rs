//! C_game housekeeping slice: sector enumeration, animation preload naming,
//! diff-program dispatch, frame-placement cleanup and inter-mission stacks.
//!
//! Reference: mafia-re pin 113b5b6, `game/mission/game.cpp` and
//! `game/mission/game.h`. Game.exe RVAs are quoted per function; the quoted
//! `game.cpp` line numbers are from that pin. No original-runtime validation
//! is claimed here.
//!
//! Marker-only contract: frames are integer handles, placements keep only the
//! compared `frame` member, and every native side effect (projector-light
//! registration, track loading, subroutine execution) runs through a caller
//! supplied backend. Device, mission and actor owners are DEFERRED and have
//! no representation in this module:
//! - `C_game::Done` (GAME 0x5a3c60, `game.cpp:1071`) needs the render device,
//!   indicators, camera, traffic, garage and actor owners.
//! - `C_game::ClearDiffData` (GAME 0x5af860, `game.cpp:3143` region) needs the
//!   mission scene owner (frame deletion) and the actor owner.
//! - `C_game::TickShoot` (GAME 0x5a8e70) needs the collision and effects
//!   owners.
//! - `BoxPlacement*` (GAME 0x5b2a00-0x5b41d0, `game.cpp:3307`) needs the box
//!   placement owner.
//! - `SaveGame*` (GAME region near `game.h:612`) needs the save-game owner.
#![forbid(unsafe_code)]

/// Opaque frame handle. Native `I3D_frame*` identity is the caller's concern.
pub type FrameId = u32;

/// Minimal placement record. The original `S_frame_placement`
/// (`game.h:226`) also stores position, rotation, scale, parent and the on
/// flag; only the compared `frame` member participates in this slice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FramePlacement {
    pub frame: FrameId,
}

/// Synthetic frame subtree used by the enumeration and cleanup callbacks.
/// The native tree is walked through `GetChild`/`NumChildren` (indexed) or
/// the `m_pChild`/`m_pNext` link list; both views agree on membership here.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HousekeepingFrame {
    pub id: FrameId,
    /// Mirrors the `GetType() == FRAME_SECTOR` check.
    pub is_sector: bool,
    pub children: Vec<HousekeepingFrame>,
}

impl HousekeepingFrame {
    pub fn leaf(id: FrameId, is_sector: bool) -> Self {
        Self {
            id,
            is_sector,
            children: Vec::new(),
        }
    }
}

/// Native `C_Vehicle::AddSectorForProjectorLights` sink for `cb_enum_sectors`.
pub trait SectorLightBackend {
    fn add_sector_for_projector_lights(&mut self, sector: FrameId);
}

/// Native `C_animations_machine_de_luxe::LoadTrack` sink for `preload_anim`.
/// The associated handle covers both original return shapes (`bool` on some
/// builds, a track handle on others); the backend decides.
pub trait AnimTrackBackend {
    type Handle;
    fn load_track(&mut self, track: &str) -> Self::Handle;
}

/// Native `C_program::CallSubroutine` sink for `global_program_run`.
/// The index identifies the matched entry of the caller's program list.
pub trait DiffProgramBackend {
    fn call_subroutine(&mut self, index: usize);
}

/// Entry of the game's own script list (`game.h:410`, `diff_programs`).
/// Program lifetime and execution stay with the mission owner; only the
/// compared `name` member participates here.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffProgram {
    pub name: String,
}

/// Marker for a kept player inventory (`game.h:439`, `G_Inventory*`).
/// The inventory object itself stays with the actor owner; dropping the
/// entry stands in for the original per-element `delete`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InventorySnapshot {
    pub marker: u32,
}

/// FUNCTION: GAME 0x5a3b90 (`game.cpp:1047`, `S_hlp::cbEnumSectors`).
/// Registers every sector frame for projector lights, then visits children
/// in reverse order, matching the original `for (i = NumChildren(); i--;)`
/// loop.
pub fn cb_enum_sectors(frame: &HousekeepingFrame, backend: &mut impl SectorLightBackend) {
    if frame.is_sector {
        backend.add_sector_for_projector_lights(frame.id);
    }
    for child in frame.children.iter().rev() {
        cb_enum_sectors(child, backend);
    }
}

/// FUNCTION: GAME 0x5a3bd0 (`game.cpp:1057`, `S_hlp::PreloadAnim`).
/// Copies the animation name, truncates it at the first `.` (if any),
/// appends `.tck` and loads that track through the backend.
/// The original copies into a fixed 128-byte buffer; this spelling takes
/// `&str` and therefore cannot overflow that buffer.
pub fn preload_anim<B: AnimTrackBackend>(backend: &mut B, name: &str) -> B::Handle {
    let stem = name.split('.').next().unwrap_or(name);
    let mut track = String::with_capacity(stem.len() + 4);
    track.push_str(stem);
    track.push_str(".tck");
    backend.load_track(&track)
}

/// FUNCTION: GAME 0x5afd50 (`game.cpp:3181`, `C_game::GlobalProgramRun`).
/// Scans the game's own scripts from the back, runs the first entry whose
/// name matches case-insensitively (`_stricmp`, approximated with ASCII
/// case folding), and reports whether one ran.
pub fn global_program_run(
    programs: &[DiffProgram],
    name: &str,
    backend: &mut impl DiffProgramBackend,
) -> bool {
    for (index, program) in programs.iter().enumerate().rev() {
        if program.name.eq_ignore_ascii_case(name) {
            backend.call_subroutine(index);
            return true;
        }
    }
    false
}

/// FUNCTION: GAME 0x5afca0 (`game.cpp:3164`, `S_hlp::cbFreeFramePlacement`).
/// Removes the single last-listed placement of `frame` (swap-remove with
/// `break`, scanning from the back), then recurses into children in reverse
/// order.
pub fn cb_free_frame_placement(frame: &HousekeepingFrame, list: &mut Vec<FramePlacement>) {
    for index in (0..list.len()).rev() {
        if list[index].frame == frame.id {
            list.swap_remove(index);
            break;
        }
    }
    for child in frame.children.iter().rev() {
        cb_free_frame_placement(child, list);
    }
}

/// FUNCTION: GAME 0x5b0210 (`game.cpp:3202`, `C_game::InvalidateFrame`).
/// A null frame is a no-op; otherwise the frame and its subtree leave the
/// placement list through `cb_del_frame`.
pub fn invalidate_frame(frame: Option<&HousekeepingFrame>, list: &mut Vec<FramePlacement>) {
    if let Some(frame) = frame {
        cb_del_frame(frame, list);
    }
}

/// FUNCTION: GAME 0x5b0230 (`game.cpp:3209`, `S_hlp::cbDelFrame`).
/// Removes every placement of `frame` (no `break`: all occurrences go),
/// then recurses only into children that themselves have children, matching
/// the original `if (child->m_pChild)` guard. A leaf child's own placements
/// therefore survive, as do the placements of the skipped subtrees.
pub fn cb_del_frame(frame: &HousekeepingFrame, list: &mut Vec<FramePlacement>) {
    for index in (0..list.len()).rev() {
        if list[index].frame == frame.id {
            list.swap_remove(index);
        }
    }
    for child in &frame.children {
        if !child.children.is_empty() {
            cb_del_frame(child, list);
        }
    }
}

/// FUNCTION: GAME 0x5b1da0 (`game.cpp:3252`, `C_game::InventoryStackClear`).
/// Drops every kept inventory snapshot (`game.h:439`); the `Vec` drain runs
/// the per-element destruction the original loop spells out with `delete`.
pub fn inventory_stack_clear(stack: &mut Vec<InventorySnapshot>) {
    stack.clear();
}

/// FUNCTION: GAME 0x5b2320 (`game.cpp:3272`, `C_game::FloatStackClear`).
pub fn float_stack_clear(stack: &mut Vec<f32>) {
    stack.clear();
}

/// FUNCTION: GAME 0x5b2590 (`game.cpp:3290`, `C_game::CharacterStackClear`).
pub fn character_stack_clear(stack: &mut Vec<u32>) {
    stack.clear();
}

/// Marker-only projection of `C_game::S_shadow_item` (mafia-re
/// `game/mission/game.h:142`).
///
/// Only the compared `dist` member (distance from the camera,
/// `game.h:150`) participates here; frame/model ownership and the shadow
/// tick stay with their owners (DEFERRED).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowItem {
    pub dist: f32,
}

/// FUNCTION: GAME 0x5acfb0 (`game.cpp:2865`, `C_game::CompareShadows`).
///
/// qsort comparator by `dist`: 0 when equal, -1 when nearer, 1 otherwise.
/// The two-comparison sequence is the whole contract; NaN behavior beyond
/// it is explicitly out of scope.
pub fn compare_shadows(a: &ShadowItem, b: &ShadowItem) -> i32 {
    if a.dist == b.dist {
        0
    } else if a.dist < b.dist {
        -1
    } else {
        1
    }
}

/// FUNCTION: GAME 0x5b6e0 (`game.cpp:3628`, `S_hlp::cbFindTaxiPoints`).
/// Records every visited frame; the original pushes the frame pointer into
/// the taxi-point list and returns `I3DENUMRET_OK` to continue enumeration.
pub fn cb_find_taxi_points(frame: &HousekeepingFrame, list: &mut Vec<FrameId>) {
    list.push(frame.id);
}

/// Marker for an owned lightness entry (mafia-re `game/mission/game.h`
/// lightness list). The original holds pointers deleted one by one; dropping
/// the entry stands in for the per-element `delete`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LightnessEntry {
    pub marker: u32,
}

/// FUNCTION: GAME 0x5b5180 (`game.cpp:3419`, `C_game::LightnessClear`).
/// Deletes every lightness entry and empties the list; the `Vec` drain runs
/// the per-element destruction the original loop spells out with `delete`.
pub fn lightness_clear(lightness: &mut Vec<LightnessEntry>) {
    lightness.clear();
}

/// Current score and its HUD switch (`game.h`, `score` / `score_on`).
/// Lifetime and rendering stay with the mission/HUD owners; only the
/// stored scalars participate here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScoreState {
    pub score: i32,
    pub score_on: bool,
}

/// Native `mafiaIndicators.BonusSetCount` sink for `ScoreSet`/`ScoreSetOn`.
pub trait HudBonusBackend {
    fn bonus_set_count(&mut self, count: i32);
}

/// FUNCTION: GAME 0x5b9fe0 (`game.cpp:3969`, `C_game::ScoreSet`).
/// Stores the score; while the switch is on the HUD bonus counter follows.
pub fn score_set(state: &mut ScoreState, value: i32, hud: &mut impl HudBonusBackend) {
    state.score = value;
    if state.score_on {
        hud.bonus_set_count(state.score);
    }
}

/// FUNCTION: GAME 0x5f9a0 (`game.cpp:3958`, `C_game::ScoreSetOn`).
/// Flips the switch; the HUD bonus counter shows the score, or `0x7fffffff`
/// (nothing) while off.
pub fn score_set_on(state: &mut ScoreState, on: bool, hud: &mut impl HudBonusBackend) {
    state.score_on = on;
    if on {
        hud.bonus_set_count(state.score);
    } else {
        hud.bonus_set_count(0x7fffffff);
    }
}

/// Opaque actor handle. Native `C_actor*` identity is the caller's concern.
pub type ActorId = u32;

fn swap_remove_one(list: &mut Vec<ActorId>, actor: ActorId) {
    for index in (0..list.len()).rev() {
        if list[index] == actor {
            list.swap_remove(index);
            break;
        }
    }
}

/// FUNCTION: GAME 0x5a79f0 (`game.cpp:2441`, `C_game::InternTempActDel`).
/// No-op when the actor is already listed as removed; otherwise
/// swap-removes its single occurrence from each of the temporary, active
/// and inactive lists (one `break` per list). The host vectors are
/// parameters because the actor owner is DEFERRED.
pub fn intern_temp_act_del(
    removed_actors: &[ActorId],
    temporary_actors: &mut Vec<ActorId>,
    active_actors: &mut Vec<ActorId>,
    inactive_actors: &mut Vec<ActorId>,
    actor: ActorId,
) {
    if removed_actors.contains(&actor) {
        return;
    }
    swap_remove_one(temporary_actors, actor);
    swap_remove_one(active_actors, actor);
    swap_remove_one(inactive_actors, actor);
}

/// Marker for a collected telephone (`game.h`, `S_Telephone`).
/// Only the compared `phone_box` flag and the source frame participate
/// here; the cache-model link stays with the scene owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TelephoneEntry {
    pub frame: FrameId,
    pub phone_box: bool,
}

/// FUNCTION: GAME 0x5b71b0 (`game.cpp:3686`, `S_hlp::cbTelephones`).
/// Pure mapping without I3D: the basename after the last `\`
/// (case-insensitive) `alarm01.i3d` yields `phone_box = false`,
/// `PHbox01.i3d` yields `true`; anything else is not a telephone and leaves
/// the list untouched. Returns whether an entry was pushed.
pub fn cb_telephones(frame: FrameId, file_name: &str, out: &mut Vec<TelephoneEntry>) -> bool {
    let base = file_name.rsplit('\\').next().unwrap_or(file_name);
    let phone_box = if base.eq_ignore_ascii_case("alarm01.i3d") {
        false
    } else if base.eq_ignore_ascii_case("PHbox01.i3d") {
        true
    } else {
        return false;
    };
    out.push(TelephoneEntry { frame, phone_box });
    true
}

/// `S_semaphore_lights` phase table (`game.cpp:90-97` at pin 113b5b6; the
/// name is a mafia-re working guess, the values are evidenced). Each entry
/// is `(red, orange, green)`: red for the first five phases, green for the
/// next four, orange for the last; the two driving directions run five
/// phases apart.
pub const SEMAPHORE_LIGHTS: [(bool, bool, bool); 10] = [
    (true, false, false),
    (true, false, false),
    (true, false, false),
    (true, false, false),
    (true, false, false),
    (false, false, true),
    (false, false, true),
    (false, false, true),
    (false, false, true),
    (false, true, false),
];

/// Which mission phase word a semaphore follows (`game.cpp:2942-2946` at pin
/// 113b5b6): the model faces along x where `|dir.x| > |dir.z|` (`phase_x`),
/// otherwise along z (`phase_z`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemaphorePhase {
    PhaseX,
    PhaseZ,
}

/// Marker for one collected semaphore (mafia-re `S_semaphore` as used at
/// `game.cpp:2935-2947`). Only the lamp frames (`None` is the native NULL)
/// and the phase selection participate here; the `FindChildFrame` lookups
/// and the scene enumeration stay with the scene owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SemaphoreEntry {
    pub red: Option<FrameId>,
    pub orange: Option<FrameId>,
    pub green: Option<FrameId>,
    pub use_phase_x: bool,
}

/// FUNCTION: GAME VA 0x5ad900 (RVA 0x1ad900; `game.cpp:2924-2949`,
/// `C_game::S_hlp::cbFindSemaphores`): pure admission half. The model-type
/// check and the `RED`/`ORANGE`/`GREEN` child lookups stay with the scene
/// owner, which calls this only for models; the basename after the last `\`
/// (case-insensitive) `semafor.i3d` is kept and mapped to its phase by the
/// model direction, anything else is skipped. Returns the phase where kept.
pub fn classify_semaphore_phase(
    file_name: &str,
    dir_x: f32,
    dir_z: f32,
) -> Option<SemaphorePhase> {
    let base = file_name.rsplit('\\').next().unwrap_or(file_name);
    if !base.eq_ignore_ascii_case("semafor.i3d") {
        return None;
    }
    if dir_x.abs() > dir_z.abs() {
        Some(SemaphorePhase::PhaseX)
    } else {
        Some(SemaphorePhase::PhaseZ)
    }
}

/// FUNCTION: GAME VA 0x5add10 (RVA 0x1add10; `game.cpp:2951-2957`,
/// `C_game::LoadSemaphores`): reset half. Empties the list; the sector
/// enumeration (`EnumFrames` with `cbFindSemaphores`) stays with the scene
/// owner.
pub fn load_semaphores_reset(semaphores: &mut Vec<SemaphoreEntry>) {
    semaphores.clear();
}

/// Native per-lamp sink for [`tick_semaphores`]: switching one lamp frame
/// on or off.
pub trait SemaphoreLightBackend {
    fn set_light(&mut self, frame: FrameId, on: bool);
}

/// FUNCTION: GAME VA 0x5ade20 (RVA 0x1ade20; `game.cpp:2975-2988`,
/// `C_game::TickSemaphores`). Applies each semaphore's direction phase from
/// the lights table, from the back like the original; absent lamps
/// (`None`) are skipped. The caller stages `phase_x`/`phase_z` in `0..10`
/// (native `Init` starts them at 0/5 and `Tick` wraps past 9 back to 0).
pub fn tick_semaphores(
    semaphores: &[SemaphoreEntry],
    phase_x: usize,
    phase_z: usize,
    backend: &mut impl SemaphoreLightBackend,
) {
    let (red_x, orange_x, green_x) = SEMAPHORE_LIGHTS[phase_x];
    let (red_z, orange_z, green_z) = SEMAPHORE_LIGHTS[phase_z];
    for entry in semaphores.iter().rev() {
        let (red, orange, green) = if entry.use_phase_x {
            (red_x, orange_x, green_x)
        } else {
            (red_z, orange_z, green_z)
        };
        if let Some(frame) = entry.red {
            backend.set_light(frame, red);
        }
        if let Some(frame) = entry.orange {
            backend.set_light(frame, orange);
        }
        if let Some(frame) = entry.green {
            backend.set_light(frame, green);
        }
    }
}

/// FUNCTION: GAME VA 0x5b6e00 (RVA 0x1b6e00; `game.cpp:3641-3648`,
/// `C_game::LoadTelephones`): reset half. Empties the list; the sector
/// enumeration runs through the already ported [`cb_telephones`], and the
/// base-cache model-set pass stays with the cache owner.
pub fn load_telephones_reset(telephones: &mut Vec<TelephoneEntry>) {
    telephones.clear();
}

/// Marker for a collected scream area (`game.h`, `S_scream_area`).
/// Only the parsed hexadecimal `id` and the source frame participate here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScreamAreaEntry {
    pub frame: FrameId,
    pub id: u16,
}

/// FUNCTION: GAME 0x5b9110 (`game.cpp:3859`, `S_hlp::cbScreamArea`).
/// Pure parser without I3D: non-area frames and names without the
/// case-insensitive `enm_scrm_` prefix (which needs more than the 9 prefix
/// characters, `strlen(name) > 9`) are skipped; otherwise the hexadecimal
/// number after `(` up to `)` (rest of the string when `)` is missing)
/// parses with `strtoul(..., 16)` semantics approximated as the leading
/// ASCII-whitespace-skipped hex-digit run, `0` without one, truncated to
/// `u16`. Returns whether an entry was pushed.
pub fn cb_scream_area(
    frame: FrameId,
    name: &str,
    is_area: bool,
    out: &mut Vec<ScreamAreaEntry>,
) -> bool {
    if !is_area {
        return false;
    }
    if name.len() <= 9 || !name[..9].eq_ignore_ascii_case("enm_scrm_") {
        return false;
    }
    let mut id: u16 = 0;
    if let Some(open) = name.find('(') {
        let rest = &name[open + 1..];
        let digits = match rest.find(')') {
            Some(close) => &rest[..close],
            None => rest,
        };
        let digits = digits.trim_start();
        let mut value: u32 = 0;
        let mut any = false;
        for byte in digits.bytes() {
            let digit = match byte {
                b'0'..=b'9' => (byte - b'0') as u32,
                b'a'..=b'f' => (byte - b'a' + 10) as u32,
                b'A'..=b'F' => (byte - b'A' + 10) as u32,
                _ => break,
            };
            any = true;
            value = value.wrapping_mul(16).wrapping_add(digit);
        }
        if any {
            // Matches the original `(ushort)` truncation: the low 16 bits
            // survive `u32` wrapping exactly.
            id = (value & 0xffff) as u16;
        }
    }
    out.push(ScreamAreaEntry { frame, id });
    true
}

/// FUNCTION: GAME 0x408450 (`C_actor.cpp:1734-1740`, free `rnd(int n)`).
/// Marker-only: `if (n < 1) return 0; return rand() % n`. The CRT `rand()`
/// call has no Rust owner here; the caller passes its `u32` result as
/// `random` and this keeps only the pure bound/guard step.
pub fn rnd_mod(random: u32, n: i32) -> i32 {
    if n < 1 {
        return 0;
    }
    (random % n as u32) as i32
}

/// FUNCTION: GAME 0x408470 (`C_actor.cpp:1742-1746`, free `rnd()`).
/// Marker-only: `rand() * (1.0f / RAND_MAX)` with MSVC `RAND_MAX = 0x7fff`.
/// The caller supplies the `rand()` result; this is the pure scaling step
/// (reciprocal then multiply, no wider intermediate).
pub fn rnd_unit(random: u32) -> f32 {
    random as f32 * (1.0f32 / 32767.0f32)
}

/// `RACE6_STATE_*` values (`game.h:47-52` at pin 113b5b6; the enumerator
/// names are mafia-re working guesses, the values are evidenced).
pub const RACE6_STATE_NONE: i32 = 0;
pub const RACE6_STATE_RUNNING: i32 = 1;
pub const RACE6_STATE_STARTED: i32 = 2;
pub const RACE6_STATE_END: i32 = 10;

/// Minimal scalar/event state behind the `C_game` inline accessors
/// (`game.h:841-961` at pin 113b5b6). The camera, using-object,
/// sledovacka, sound-arena and shvestky objects stay with their owners;
/// only the scalar, flag, handle and event-pair fields these inlines read
/// or write are owned here.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameInlineState {
    player: Option<ActorId>,
    clear_diff_data: bool,
    game_time: i32,
    init_frames: u32,
    timeout_program: u32,
    timeout_event: i32,
    player_fire_program: u32,
    player_fire_event: i32,
    player_horn_program: u32,
    player_horn_event: i32,
    player_wfall_program: u32,
    player_wfall_event: i32,
    mission_end: i32,
    race6_state: i32,
    mission_passed: bool,
}

impl GameInlineState {
    /// FUNCTION: GAME 0x47b520 (RVA 0x7b520, VA 0x47b520; `game.h:848-851`,
    /// `C_game::GetPlayer`): reads the player staged by `Init` (the
    /// mission's last `ACTOR_PLAYER`). `None` is the native NULL.
    pub fn get_player(&self) -> Option<ActorId> {
        self.player
    }

    /// Staging for [`GameInlineState::get_player`]; the `Init` selection
    /// itself stays with the mission owner.
    pub fn set_player(&mut self, player: Option<ActorId>) {
        self.player = player;
    }

    /// FUNCTION: GAME 0x47b540 (RVA 0x7b540, VA 0x47b540; `game.h:857-862`,
    /// `C_game::SetClearDiffData`): sets the cleared-differences flag
    /// (`CLEARDIFFERENCES`'; PS2's does nothing).
    pub fn set_clear_diff_data(&mut self) {
        self.clear_diff_data = true;
    }

    /// Reads the flag written by [`GameInlineState::set_clear_diff_data`].
    pub fn clear_diff_data(&self) -> bool {
        self.clear_diff_data
    }

    /// FUNCTION: GAME 0x47b560 (RVA 0x7b560, VA 0x47b560; `game.h:868-873`,
    /// `C_game::GetGameTime`): reads the game's time.
    pub fn get_game_time(&self) -> i32 {
        self.game_time
    }

    /// Staging for [`GameInlineState::get_game_time`]; the `Tick` advance
    /// itself stays with the game-tick owner.
    pub fn set_game_time(&mut self, game_time: i32) {
        self.game_time = game_time;
    }

    /// FUNCTION: GAME 0x47b570 (RVA 0x7b570, VA 0x47b570; `game.h:874-878`,
    /// `C_game::GetInitFrames`): reads the frames counted since `Init`.
    pub fn get_init_frames(&self) -> u32 {
        self.init_frames
    }

    /// Staging for [`GameInlineState::get_init_frames`]; the `Tick`
    /// increment itself stays with the game-tick owner.
    pub fn set_init_frames(&mut self, init_frames: u32) {
        self.init_frames = init_frames;
    }

    /// FUNCTION: GAME 0x47b5a0 (RVA 0x7b5a0, VA 0x47b5a0; `game.h:888-894`,
    /// `C_game::SetTimeOutEvent`): stores the timeout script and event.
    pub fn set_timeout_event(&mut self, program: u32, event: i32) {
        self.timeout_program = program;
        self.timeout_event = event;
    }

    /// Reads the pair written by [`GameInlineState::set_timeout_event`].
    pub fn timeout_event(&self) -> (u32, i32) {
        (self.timeout_program, self.timeout_event)
    }

    /// FUNCTION: GAME 0x47b5c0 (RVA 0x7b5c0, VA 0x47b5c0; `game.h:895-900`,
    /// `C_game::SetPlayerFireEvent`): stores the player-fire script and event.
    pub fn set_player_fire_event(&mut self, program: u32, event: i32) {
        self.player_fire_program = program;
        self.player_fire_event = event;
    }

    /// Reads the pair written by [`GameInlineState::set_player_fire_event`].
    pub fn player_fire_event(&self) -> (u32, i32) {
        (self.player_fire_program, self.player_fire_event)
    }

    /// FUNCTION: GAME 0x47b5e0 (RVA 0x7b5e0, VA 0x47b5e0; `game.h:901-906`,
    /// `C_game::SetPlayerHornEvent`): stores the player-horn script and event.
    pub fn set_player_horn_event(&mut self, program: u32, event: i32) {
        self.player_horn_program = program;
        self.player_horn_event = event;
    }

    /// Reads the pair written by [`GameInlineState::set_player_horn_event`].
    pub fn player_horn_event(&self) -> (u32, i32) {
        (self.player_horn_program, self.player_horn_event)
    }

    /// FUNCTION: GAME 0x47b600 (RVA 0x7b600, VA 0x47b600; `game.h:907-912`,
    /// `C_game::SetPlayerWFallEvent`): stores the player-fall script and event.
    pub fn set_player_wfall_event(&mut self, program: u32, event: i32) {
        self.player_wfall_program = program;
        self.player_wfall_event = event;
    }

    /// Reads the pair written by [`GameInlineState::set_player_wfall_event`].
    pub fn player_wfall_event(&self) -> (u32, i32) {
        (self.player_wfall_program, self.player_wfall_event)
    }

    /// FUNCTION: GAME 0x47b630 (RVA 0x7b630, VA 0x47b630; `game.h:917-922`,
    /// `C_game::SetMissionEnd`): stores how the mission ends (1 on
    /// `ENDOFMISSION`, 2 on failure-menu exit/retry).
    pub fn set_mission_end(&mut self, value: i32) {
        self.mission_end = value;
    }

    /// Reads the value written by [`GameInlineState::set_mission_end`].
    pub fn mission_end(&self) -> i32 {
        self.mission_end
    }

    /// FUNCTION: GAME 0x47b640 (RVA 0x7b640, VA 0x47b640; `game.h:922-927`,
    /// `C_game::SetRace6State`): `start` selects `RACING_MISSION6_START`'s
    /// `RACE6_STATE_STARTED`, otherwise `RACE6_STATE_NONE`.
    pub fn set_race6_state(&mut self, start: bool) {
        self.race6_state = if start {
            RACE6_STATE_STARTED
        } else {
            RACE6_STATE_NONE
        };
    }

    /// FUNCTION: GAME 0x47b660 (RVA 0x7b660, VA 0x47b660; `game.h:927-932`,
    /// `C_game::GetRace6State`): reads the race state.
    pub fn get_race6_state(&self) -> i32 {
        self.race6_state
    }

    /// FUNCTION: GAME 0x47b6c0 (RVA 0x7b6c0, VA 0x47b6c0; `game.h:957-961`,
    /// `C_game::SetMissionPassed`): stores whether the mission passed
    /// (`ENDOFMISSION`'s, cleared by `Init`); the garage's `GarageGameDone`
    /// lets the player choose the cars while it is set.
    pub fn set_mission_passed(&mut self, value: bool) {
        self.mission_passed = value;
    }

    /// Reads the gate written by [`GameInlineState::set_mission_passed`].
    pub fn mission_passed(&self) -> bool {
        self.mission_passed
    }
}

/// STUB: GAME 0x5b7500 (RVA 0x1b7500, VA 0x5b7500; `game.cpp:3710-3715`,
/// `game.h:803-804`, `C_game::IsTelephoneExist`): whether the frame is one
/// of the telephones. The native body is unwritten (always false); the host
/// implements the named contract over the collected [`TelephoneEntry`]
/// list: linear scan, first match wins.
pub fn is_telephone_exist(telephones: &[TelephoneEntry], frame: FrameId) -> bool {
    telephones.iter().any(|entry| entry.frame == frame)
}

/// Explicit boundary for referenced-but-unwritten Game.exe entries: the
/// reference names the contract, but no original body is available, so the
/// host reports the entry instead of guessing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnwrittenGameEntry {
    Unsupported { entry: u32 },
}

impl core::fmt::Display for UnwrittenGameEntry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            UnwrittenGameEntry::Unsupported { entry } => {
                write!(f, "unwritten Game.exe entry {entry:#x}")
            }
        }
    }
}

impl std::error::Error for UnwrittenGameEntry {}

/// STUB: GAME 0x481840 (RVA 0x81840, VA 0x481840; `C_pumper.cpp:15-20`,
/// `C_pumper.h:31-32`, `C_pumper::ScriptCanPump`): whether the mission's
/// fuel-pump attendant of the id can pump (the script's float register gets
/// its int). Unwritten upstream; explicit unsupported boundary.
pub const SCRIPT_CAN_PUMP_ENTRY: u32 = 0x481840;

/// Original `C_pumper::ScriptCanPump` stub: always reports the boundary,
/// mirroring the native zero without inventing pump semantics.
pub fn script_can_pump(_attendant: u32) -> Result<i32, UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: SCRIPT_CAN_PUMP_ENTRY,
    })
}

/// STUB: GAME 0x4ba300 (RVA 0xba300, VA 0x4ba300; `C_trf_c.cpp:187-192`,
/// `C_trf_c.h:50-52`, `C_traffic_car::SebraliMiKaaaruSvine`): the traffic
/// car the human gets into (NULL where he cannot have it). Unwritten
/// upstream; explicit unsupported boundary.
pub const SEBRALI_MI_KAARU_SVINE_ENTRY: u32 = 0x4ba300;

/// Original `C_traffic_car::SebraliMiKaaaruSvine` stub: always reports the
/// boundary, mirroring the native NULL without inventing the hand-off.
pub fn sebrali_mi_kaaaru_svine(_car: ActorId) -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: SEBRALI_MI_KAARU_SVINE_ENTRY,
    })
}

/// Marker for one road block (`C_game::S_RoadBlock`, `game.h:118-124`).
/// Only the stored `node` (roads' nearest node, `0xffff` for none) and the
/// `active` flag participate here; the `unknown_4` words stay with the roads
/// owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoadBlockEntry {
    pub node: u16,
    pub active: bool,
}

/// FUNCTION: GAME RVA 0x1b69d0, VA 0x5b69d0 (`game.cpp:3595-3602`,
/// `C_game::LoadRoadBlocks`): reset half. Empties the list; the sector
/// enumeration (`EnumFrames` with `"ROADBLOCK*"`) stays with the scene owner.
/// Verified: Game.exe SHA256 `0839...ab27` `.text` entry bytes read the
/// `road_blocks` vector at `game.h:340`.
pub fn load_road_blocks_reset(blocks: &mut Vec<RoadBlockEntry>) {
    blocks.clear();
}

/// FUNCTION: GAME RVA 0x1b6a50, VA 0x5b6a50 (`game.cpp:3605-3614`,
/// `C_game::S_hlp::cbRoadBlock`): admission half. A block is kept only where
/// the roads owner found a node (`node != 0xffff`); the kept entry starts
/// inactive (`active = false`, raised later by the police owner). The
/// `GetNearNode` world-position lookup stays with the roads owner, so the
/// caller passes its result in. Returns whether an entry was pushed.
/// Verified: `.text` entry with the vector push-if-valid shape.
pub fn collect_road_block(node: u16, out: &mut Vec<RoadBlockEntry>) -> bool {
    if node == 0xffff {
        return false;
    }
    out.push(RoadBlockEntry { node, active: false });
    true
}

/// FUNCTION: GAME RVA 0x1b6b10, VA 0x5b6b10 (`game.cpp:3617-3625`,
/// `C_game::LoadTaxiPoints`): reset half. Empties the list; the sector
/// enumeration (`EnumFrames` with `"taxi_point*"`) stays with the scene
/// owner. Verified: `.text` entry reads the `taxi_points` vector cap at
/// `game.h:346`.
pub fn load_taxi_points_reset(points: &mut Vec<FrameId>) {
    points.clear();
}

/// FUNCTION: GAME RVA 0x1b90b0, VA 0x5b90b0 (`game.cpp:3850-3855`,
/// `C_game::LoadScreamAreas`): reset half. Empties the list; the sector
/// enumeration runs through the already ported [`cb_scream_area`].
/// Verified: `.text` entry reads the `scream_areas` vector cap at
/// `game.h:505`.
pub fn load_scream_areas_reset(areas: &mut Vec<ScreamAreaEntry>) {
    areas.clear();
}

/// Traffic-sign model sets collected by `CreateSignalMarkIDX`
/// (`game.cpp:3736-3769`): `dopravni1B.I3D` through `dopravni8B.I3D`,
/// matched case-insensitively like the original `_stricmp` chain.
const SIGNAL_MARK_MODELS: [&str; 8] = [
    "dopravni1B.I3D",
    "dopravni2B.I3D",
    "dopravni3B.I3D",
    "dopravni4B.I3D",
    "dopravni5B.I3D",
    "dopravni6B.I3D",
    "dopravni7B.I3D",
    "dopravni8B.I3D",
];

/// Fragment of GAME RVA 0x1b7860 (no own entry; part of
/// `C_game::CreateSignalMarkIDX`): whether a base-cache model-set name is
/// one of the eight traffic-sign sets.
pub fn signal_mark_model_matches(name: &str) -> bool {
    SIGNAL_MARK_MODELS.iter().any(|candidate| candidate.eq_ignore_ascii_case(name))
}

/// FUNCTION: GAME RVA 0x1b7860, VA 0x5b7860 (`game.cpp:3736-3769`,
/// `C_game::CreateSignalMarkIDX`): pure collection half. Scans the model-set
/// names from the back and pushes the index of every traffic-sign set, so
/// the result runs from the highest index down, as in the original.
/// The base-cache owner stays with the caller, which passes the names in.
/// Verified: `.text` entry reads the `signal_mark_idx` vector cap at
/// `game.h:345`.
pub fn collect_signal_mark_indices(model_sets: &[impl AsRef<str>]) -> Vec<u16> {
    let mut indices = Vec::new();
    for (index, name) in model_sets.iter().enumerate().rev() {
        if signal_mark_model_matches(name.as_ref()) {
            indices.push(index as u16);
        }
    }
    indices
}

/// Marker for one hydrant water entry (`C_game::S_hydrant`,
/// `game.h:152-160`). Only the compared `particle`, the released `frame`
/// (`None` is the native NULL) and the stored order participate here; the
/// `blocked`/`pos` tick reads stay with the tick owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HydrantEntry {
    pub particle: i32,
    pub frame: Option<FrameId>,
}

/// Native effects/frame sink for [`del_all_hydrants`]: destroying the
/// particle, then the `SetOn(false)` + `Release` pair for a present frame.
pub trait HydrantBackend {
    fn destroy_particle(&mut self, particle: i32);
    fn release_frame(&mut self, frame: FrameId);
}

/// FUNCTION: GAME RVA 0x1b9ef0, VA 0x5b9ef0 (`game.cpp:3941-3954`,
/// `C_game::DelAllHydrants`). Destroys every hydrant particle from the back,
/// releases every present frame through the backend, then empties the list.
/// Verified: `.text` entry reads the `hydrants` vector at `game.h:362`.
pub fn del_all_hydrants(hydrants: &mut Vec<HydrantEntry>, backend: &mut impl HydrantBackend) {
    for index in (0..hydrants.len()).rev() {
        let entry = hydrants[index];
        backend.destroy_particle(entry.particle);
        if let Some(frame) = entry.frame {
            backend.release_frame(frame);
        }
    }
    hydrants.clear();
}

/// STUB: GAME RVA 0x1b6d10, VA 0x5b6d10 (`game.cpp:3634-3639`,
/// `C_game::IsValidTaxiPassenger`): the native body is unwritten (`StubLog`
/// + `false`). The taxi-passenger human and the car/seat checks stay with
/// the actor owner. Verified: `.text` entry loads the `taxi_passenger`
/// field at `game.h:347` (`mov eax,[ecx+0x204]`) before the null test.
pub fn is_valid_taxi_passenger() -> bool {
    false
}

/// STUB: GAME RVA 0x1b7560, VA 0x5b7560 (`game.cpp:3717-3721`,
/// `C_game::AddHumanTalk`): unwritten upstream; the human owner is deferred.
pub const ADD_HUMAN_TALK_ENTRY: u32 = 0x5b7560;

/// Original `C_game::AddHumanTalk` stub: always reports the boundary.
pub fn add_human_talk(_human: ActorId, _channel: i32) -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: ADD_HUMAN_TALK_ENTRY,
    })
}

/// STUB: GAME RVA 0x1b7730, VA 0x5b7730 (`game.cpp:3723-3727`,
/// `C_game::DelHumanTalk`): unwritten upstream; the human owner is deferred.
pub const DEL_HUMAN_TALK_ENTRY: u32 = 0x5b7730;

/// Original `C_game::DelHumanTalk` stub: always reports the boundary.
pub fn del_human_talk(_human: ActorId, _channel: i32) -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: DEL_HUMAN_TALK_ENTRY,
    })
}

/// STUB: GAME RVA 0x1b77c0, VA 0x5b77c0 (`game.cpp:3729-3733`,
/// `C_game::DeathHumanTalk`): unwritten upstream; the human owner is
/// deferred.
pub const DEATH_HUMAN_TALK_ENTRY: u32 = 0x5b77c0;

/// Original `C_game::DeathHumanTalk` stub: always reports the boundary.
pub fn death_human_talk(_human: ActorId) -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: DEATH_HUMAN_TALK_ENTRY,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct SectorLog {
        visited: Vec<FrameId>,
    }

    impl SectorLightBackend for SectorLog {
        fn add_sector_for_projector_lights(&mut self, sector: FrameId) {
            self.visited.push(sector);
        }
    }

    struct TrackLog {
        loaded: Vec<String>,
    }

    impl AnimTrackBackend for TrackLog {
        type Handle = bool;

        fn load_track(&mut self, track: &str) -> bool {
            self.loaded.push(track.to_owned());
            true
        }
    }

    struct SubroutineLog {
        called: Vec<usize>,
    }

    impl DiffProgramBackend for SubroutineLog {
        fn call_subroutine(&mut self, index: usize) {
            self.called.push(index);
        }
    }

    fn placement(frame: FrameId) -> FramePlacement {
        FramePlacement { frame }
    }

    #[test]
    fn enum_sectors_registers_sectors_children_reversed() {
        let root = HousekeepingFrame {
            id: 1,
            is_sector: false,
            children: vec![
                HousekeepingFrame::leaf(2, true),
                HousekeepingFrame {
                    id: 3,
                    is_sector: true,
                    children: vec![HousekeepingFrame::leaf(4, true)],
                },
            ],
        };
        let mut backend = SectorLog { visited: vec![] };
        cb_enum_sectors(&root, &mut backend);
        assert_eq!(backend.visited, vec![3, 4, 2]);
    }

    #[test]
    fn preload_anim_replaces_extension_with_tck() {
        let mut backend = TrackLog { loaded: vec![] };
        for (name, expected) in [
            ("walk.ani", "walk.tck"),
            ("walk", "walk.tck"),
            ("a.b.c", "a.tck"),
            ("", ".tck"),
        ] {
            assert!(preload_anim(&mut backend, name));
            assert_eq!(backend.loaded.last().unwrap(), expected);
        }
    }

    #[test]
    fn global_program_run_matches_last_case_insensitive_entry() {
        let programs = vec![
            DiffProgram {
                name: "Idle".to_owned(),
            },
            DiffProgram {
                name: "GameInit".to_owned(),
            },
            DiffProgram {
                name: "gameinit".to_owned(),
            },
        ];
        let mut backend = SubroutineLog { called: vec![] };
        assert!(global_program_run(&programs, "GAMEINIT", &mut backend));
        assert_eq!(backend.called, vec![2]);
    }

    #[test]
    fn global_program_run_reports_missing_program() {
        let mut backend = SubroutineLog { called: vec![] };
        assert!(!global_program_run(&[], "GameInit", &mut backend));
        let programs = vec![DiffProgram {
            name: "Idle".to_owned(),
        }];
        assert!(!global_program_run(&programs, "GameInit", &mut backend));
        assert!(backend.called.is_empty());
    }

    #[test]
    fn free_frame_placement_removes_single_occurrence_and_recurses() {
        let root = HousekeepingFrame {
            id: 7,
            is_sector: false,
            children: vec![HousekeepingFrame::leaf(9, false)],
        };
        let mut list = vec![placement(7), placement(9), placement(7)];
        cb_free_frame_placement(&root, &mut list);
        assert_eq!(list, vec![placement(7)]);
    }

    #[test]
    fn del_frame_removes_all_occurrences_but_skips_leaf_children() {
        let root = HousekeepingFrame {
            id: 7,
            is_sector: false,
            children: vec![
                HousekeepingFrame::leaf(9, false),
                HousekeepingFrame {
                    id: 11,
                    is_sector: false,
                    children: vec![HousekeepingFrame::leaf(13, false)],
                },
            ],
        };
        let mut list = vec![
            placement(7),
            placement(9),
            placement(7),
            placement(11),
            placement(13),
        ];
        cb_del_frame(&root, &mut list);
        // Both occurrences of the root and the non-leaf child go; the leaf
        // child (9) and the grandchild leaf (13) keep their placements,
        // matching the `if (child->m_pChild)` guard at `game.cpp:3221`.
        // Swap-remove order is significant: the trailing survivor moves first.
        assert_eq!(list, vec![placement(13), placement(9)]);
    }

    #[test]
    fn invalidate_frame_null_is_noop_and_some_delegates() {
        let mut list = vec![placement(7)];
        invalidate_frame(None, &mut list);
        assert_eq!(list, vec![placement(7)]);

        let frame = HousekeepingFrame::leaf(7, false);
        invalidate_frame(Some(&frame), &mut list);
        assert!(list.is_empty());
    }

    #[test]
    fn stacks_clear() {
        let mut inventory = vec![
            InventorySnapshot { marker: 1 },
            InventorySnapshot { marker: 2 },
        ];
        inventory_stack_clear(&mut inventory);
        assert!(inventory.is_empty());

        let mut floats = vec![1.0f32, 2.0];
        float_stack_clear(&mut floats);
        assert!(floats.is_empty());

        let mut characters = vec![3u32, 4];
        character_stack_clear(&mut characters);
        assert!(characters.is_empty());
    }

    #[test]
    fn compare_shadows_orders_by_distance() {
        let near = ShadowItem { dist: 1.0 };
        let far = ShadowItem { dist: 2.0 };
        assert_eq!(compare_shadows(&near, &near), 0);
        assert_eq!(compare_shadows(&near, &far), -1);
        assert_eq!(compare_shadows(&far, &near), 1);
    }

    #[test]
    fn find_taxi_points_pushes_every_frame() {
        let frame = HousekeepingFrame::leaf(5, false);
        let mut list = vec![1];
        cb_find_taxi_points(&frame, &mut list);
        assert_eq!(list, vec![1, 5]);
    }

    #[test]
    fn lightness_clear_empties() {
        let mut lightness = vec![LightnessEntry { marker: 1 }, LightnessEntry { marker: 2 }];
        lightness_clear(&mut lightness);
        assert!(lightness.is_empty());
    }

    struct BonusLog {
        counts: Vec<i32>,
    }

    impl HudBonusBackend for BonusLog {
        fn bonus_set_count(&mut self, count: i32) {
            self.counts.push(count);
        }
    }

    #[test]
    fn score_set_stores_and_updates_hud_only_when_on() {
        let mut hud = BonusLog { counts: vec![] };
        let mut state = ScoreState {
            score: 0,
            score_on: false,
        };
        score_set(&mut state, 42, &mut hud);
        assert_eq!(state.score, 42);
        assert!(hud.counts.is_empty());

        state.score_on = true;
        score_set(&mut state, 7, &mut hud);
        assert_eq!(state.score, 7);
        assert_eq!(hud.counts, vec![7]);
    }

    #[test]
    fn score_set_on_toggles_hud_counter() {
        let mut hud = BonusLog { counts: vec![] };
        let mut state = ScoreState {
            score: 11,
            score_on: false,
        };
        score_set_on(&mut state, true, &mut hud);
        assert!(state.score_on);
        assert_eq!(hud.counts, vec![11]);
        score_set_on(&mut state, false, &mut hud);
        assert!(!state.score_on);
        assert_eq!(hud.counts, vec![11, 0x7fffffff]);
    }

    #[test]
    fn intern_temp_act_del_swap_removes_single_occurrence() {
        let removed: Vec<ActorId> = vec![];
        let mut temporary = vec![1, 2, 3];
        let mut active = vec![2, 4];
        let mut inactive = vec![5, 2, 6];
        intern_temp_act_del(&removed, &mut temporary, &mut active, &mut inactive, 2);
        // Swap-remove: the trailing element moves into the hole.
        assert_eq!(temporary, vec![1, 3]);
        assert_eq!(active, vec![4]);
        assert_eq!(inactive, vec![5, 6]);
    }

    #[test]
    fn intern_temp_act_del_skips_removed_actor() {
        let removed = vec![9];
        let mut temporary = vec![9];
        let mut active = vec![9];
        let mut inactive = vec![9];
        intern_temp_act_del(&removed, &mut temporary, &mut active, &mut inactive, 9);
        assert_eq!(temporary, vec![9]);
        assert_eq!(active, vec![9]);
        assert_eq!(inactive, vec![9]);
    }

    #[test]
    fn telephones_map_models_case_insensitively() {
        let mut out = vec![];
        assert!(cb_telephones(1, "models\\alarm01.i3d", &mut out));
        assert!(cb_telephones(2, "PHBOX01.I3D", &mut out));
        assert!(!cb_telephones(3, "models\\other.i3d", &mut out));
        assert_eq!(
            out,
            vec![
                TelephoneEntry {
                    frame: 1,
                    phone_box: false,
                },
                TelephoneEntry {
                    frame: 2,
                    phone_box: true,
                },
            ]
        );
    }

    #[test]
    fn scream_area_parses_hex_id_or_zero() {
        let mut out = vec![];
        assert!(cb_scream_area(1, "enm_scrm_downtown(2A)", true, &mut out));
        assert!(cb_scream_area(2, "ENM_SCRM_park", true, &mut out));
        assert!(!cb_scream_area(3, "enm_scrm_park(1)", false, &mut out));
        assert!(!cb_scream_area(4, "other(5)", true, &mut out));
        assert!(!cb_scream_area(5, "enm_scrm_", true, &mut out));
        assert_eq!(
            out,
            vec![
                ScreamAreaEntry { frame: 1, id: 0x2a },
                ScreamAreaEntry { frame: 2, id: 0 },
            ]
        );
    }

    #[test]
    fn rva_1ad900_classify_semaphore_phase_matches_model_and_direction() {
        // C_game::S_hlp::cbFindSemaphores GAME RVA 0x1ad900 (VA 0x5ad900).
        assert_eq!(
            classify_semaphore_phase("models\\semafor.i3d", 1.0, 0.0),
            Some(SemaphorePhase::PhaseX)
        );
        assert_eq!(
            classify_semaphore_phase("SEMAFOR.I3D", 0.0, 1.0),
            Some(SemaphorePhase::PhaseZ)
        );
        // Equal components take the z branch (`>` is strict).
        assert_eq!(
            classify_semaphore_phase("semafor.i3d", 1.0, 1.0),
            Some(SemaphorePhase::PhaseZ)
        );
        assert_eq!(
            classify_semaphore_phase("semafor.i3d", -3.0, 2.0),
            Some(SemaphorePhase::PhaseX)
        );
        assert_eq!(classify_semaphore_phase("models\\alarm01.i3d", 5.0, 0.0), None);
        assert_eq!(classify_semaphore_phase("other.i3d", 0.0, 0.0), None);
        // Basename matching is case-insensitive past the last backslash.
        assert_eq!(
            classify_semaphore_phase("x\\SemAfor.I3D", 0.0, -2.0),
            Some(SemaphorePhase::PhaseZ)
        );
    }

    #[test]
    fn rva_1add10_load_semaphores_reset_empties() {
        // C_game::LoadSemaphores GAME RVA 0x1add10 (VA 0x5add10): reset half.
        let mut semaphores = vec![SemaphoreEntry {
            red: Some(1),
            orange: None,
            green: Some(3),
            use_phase_x: true,
        }];
        load_semaphores_reset(&mut semaphores);
        assert!(semaphores.is_empty());
        load_semaphores_reset(&mut semaphores);
        assert!(semaphores.is_empty());
    }

    struct SemaphoreLog {
        states: Vec<(FrameId, bool)>,
    }

    impl SemaphoreLightBackend for SemaphoreLog {
        fn set_light(&mut self, frame: FrameId, on: bool) {
            self.states.push((frame, on));
        }
    }

    #[test]
    fn rva_1ade20_tick_semaphores_applies_phase_table_back_to_front() {
        // C_game::TickSemaphores GAME RVA 0x1ade20 (VA 0x5ade20).
        assert_eq!(SEMAPHORE_LIGHTS[0], (true, false, false));
        assert_eq!(SEMAPHORE_LIGHTS[9], (false, true, false));
        let semaphores = vec![
            SemaphoreEntry {
                red: Some(1),
                orange: Some(2),
                green: Some(3),
                use_phase_x: true,
            },
            SemaphoreEntry {
                red: None,
                orange: Some(4),
                green: Some(5),
                use_phase_x: false,
            },
        ];
        // phase_x = 0 is red, phase_z = 5 is green; the back entry runs first.
        let mut backend = SemaphoreLog { states: vec![] };
        tick_semaphores(&semaphores, 0, 5, &mut backend);
        assert_eq!(
            backend.states,
            vec![(4, false), (5, true), (1, true), (2, false), (3, false)]
        );
        // The orange phase lights only the orange lamp.
        let mut backend = SemaphoreLog { states: vec![] };
        tick_semaphores(&semaphores, 9, 9, &mut backend);
        assert_eq!(
            backend.states,
            vec![(4, true), (5, false), (1, false), (2, true), (3, false)]
        );
        // Empty input drives no lamp.
        let mut backend = SemaphoreLog { states: vec![] };
        tick_semaphores(&[], 0, 5, &mut backend);
        assert!(backend.states.is_empty());
    }

    #[test]
    fn rva_1b6e00_load_telephones_reset_empties() {
        // C_game::LoadTelephones GAME RVA 0x1b6e00 (VA 0x5b6e00): reset half.
        let mut telephones = vec![TelephoneEntry {
            frame: 1,
            phone_box: false,
        }];
        load_telephones_reset(&mut telephones);
        assert!(telephones.is_empty());
        load_telephones_reset(&mut telephones);
        assert!(telephones.is_empty());
    }

    #[test]
    fn rnd_mod_rejects_non_positive_bound() {
        assert_eq!(rnd_mod(12345, 0), 0);
        assert_eq!(rnd_mod(12345, -7), 0);
        assert_eq!(rnd_mod(12345, i32::MIN), 0);
    }

    #[test]
    fn rnd_mod_stays_in_range_and_is_deterministic() {
        for random in 0..2000u32 {
            let value = rnd_mod(random, 10);
            assert!((0..10).contains(&value));
        }
        assert_eq!(rnd_mod(7, 10), rnd_mod(7, 10));
        assert_eq!(rnd_mod(7, 10), 7);
        assert_eq!(rnd_mod(32767, 10), 7);
        assert_eq!(rnd_mod(5, 1), 0);
    }

    #[test]
    fn rnd_unit_scales_rand_max_to_one() {
        assert_eq!(rnd_unit(0), 0.0);
        let unit = rnd_unit(0x7fff);
        assert!((unit - 1.0).abs() < 1e-6, "{unit}");
        for random in [1u32, 100, 16383, 32767] {
            let value = rnd_unit(random);
            assert!((0.0..=1.0).contains(&value), "{value}");
        }
    }

    #[test]
    fn rva_7b520_get_player_returns_staged_handle() {
        // C_game::GetPlayer GAME RVA 0x7b520 (VA 0x47b520).
        let mut state = GameInlineState::default();
        assert_eq!(state.get_player(), None);
        state.set_player(Some(9));
        assert_eq!(state.get_player(), Some(9));
        state.set_player(None);
        assert_eq!(state.get_player(), None);
    }

    #[test]
    fn rva_7b540_set_clear_diff_data_sets_flag() {
        // C_game::SetClearDiffData GAME RVA 0x7b540 (VA 0x47b540).
        let mut state = GameInlineState::default();
        assert!(!state.clear_diff_data());
        state.set_clear_diff_data();
        assert!(state.clear_diff_data());
    }

    #[test]
    fn rva_7b560_get_game_time_returns_staged_time() {
        // C_game::GetGameTime GAME RVA 0x7b560 (VA 0x47b560).
        let mut state = GameInlineState::default();
        assert_eq!(state.get_game_time(), 0);
        state.set_game_time(-42);
        assert_eq!(state.get_game_time(), -42);
        state.set_game_time(i32::MAX);
        assert_eq!(state.get_game_time(), i32::MAX);
    }

    #[test]
    fn rva_7b570_get_init_frames_returns_staged_count() {
        // C_game::GetInitFrames GAME RVA 0x7b570 (VA 0x47b570).
        let mut state = GameInlineState::default();
        assert_eq!(state.get_init_frames(), 0);
        state.set_init_frames(7);
        assert_eq!(state.get_init_frames(), 7);
        state.set_init_frames(u32::MAX);
        assert_eq!(state.get_init_frames(), u32::MAX);
    }

    #[test]
    fn rva_7b5a0_set_timeout_event_stores_pair() {
        // C_game::SetTimeOutEvent GAME RVA 0x7b5a0 (VA 0x47b5a0).
        let mut state = GameInlineState::default();
        assert_eq!(state.timeout_event(), (0, 0));
        state.set_timeout_event(3, -1);
        assert_eq!(state.timeout_event(), (3, -1));
    }

    #[test]
    fn rva_7b5c0_set_player_fire_event_stores_pair() {
        // C_game::SetPlayerFireEvent GAME RVA 0x7b5c0 (VA 0x47b5c0).
        let mut state = GameInlineState::default();
        state.set_player_fire_event(11, 22);
        assert_eq!(state.player_fire_event(), (11, 22));
    }

    #[test]
    fn rva_7b5e0_set_player_horn_event_stores_pair() {
        // C_game::SetPlayerHornEvent GAME RVA 0x7b5e0 (VA 0x47b5e0).
        let mut state = GameInlineState::default();
        state.set_player_horn_event(5, 6);
        assert_eq!(state.player_horn_event(), (5, 6));
    }

    #[test]
    fn rva_7b600_set_player_wfall_event_stores_pair() {
        // C_game::SetPlayerWFallEvent GAME RVA 0x7b600 (VA 0x47b600).
        let mut state = GameInlineState::default();
        state.set_player_wfall_event(u32::MAX, i32::MIN);
        assert_eq!(state.player_wfall_event(), (u32::MAX, i32::MIN));
    }

    #[test]
    fn rva_7b630_set_mission_end_stores_value() {
        // C_game::SetMissionEnd GAME RVA 0x7b630 (VA 0x47b630).
        let mut state = GameInlineState::default();
        assert_eq!(state.mission_end(), 0);
        state.set_mission_end(1);
        assert_eq!(state.mission_end(), 1);
        state.set_mission_end(2);
        assert_eq!(state.mission_end(), 2);
    }

    #[test]
    fn rva_7b640_set_race6_state_maps_bool_to_started_or_none() {
        // C_game::SetRace6State GAME RVA 0x7b640 (VA 0x47b640).
        assert_eq!(RACE6_STATE_NONE, 0);
        assert_eq!(RACE6_STATE_RUNNING, 1);
        assert_eq!(RACE6_STATE_STARTED, 2);
        assert_eq!(RACE6_STATE_END, 10);
        let mut state = GameInlineState::default();
        state.set_race6_state(false);
        assert_eq!(state.get_race6_state(), RACE6_STATE_NONE);
        state.set_race6_state(true);
        assert_eq!(state.get_race6_state(), RACE6_STATE_STARTED);
    }

    #[test]
    fn rva_7b660_get_race6_state_returns_stored_state() {
        // C_game::GetRace6State GAME RVA 0x7b660 (VA 0x47b660).
        let state = GameInlineState::default();
        assert_eq!(state.get_race6_state(), RACE6_STATE_NONE);
    }

    #[test]
    fn rva_7b6c0_set_mission_passed_stores_garage_gate() {
        // C_game::SetMissionPassed GAME RVA 0x7b6c0 (VA 0x47b6c0).
        let mut state = GameInlineState::default();
        assert!(!state.mission_passed());
        state.set_mission_passed(true);
        assert!(state.mission_passed());
        state.set_mission_passed(false);
        assert!(!state.mission_passed());
    }

    #[test]
    fn rva_1b7500_is_telephone_exist_matches_by_frame() {
        // C_game::IsTelephoneExist GAME RVA 0x1b7500 (VA 0x5b7500).
        let telephones = vec![
            TelephoneEntry {
                frame: 1,
                phone_box: false,
            },
            TelephoneEntry {
                frame: 2,
                phone_box: true,
            },
        ];
        assert!(is_telephone_exist(&telephones, 1));
        assert!(is_telephone_exist(&telephones, 2));
        assert!(!is_telephone_exist(&telephones, 3));
        let empty: Vec<TelephoneEntry> = vec![];
        assert!(!is_telephone_exist(&empty, 1));
    }

    #[test]
    fn rva_81840_script_can_pump_reports_unsupported() {
        // C_pumper::ScriptCanPump GAME RVA 0x81840 (VA 0x481840): STUB.
        assert_eq!(SCRIPT_CAN_PUMP_ENTRY, 0x481840);
        assert_eq!(
            script_can_pump(7),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x481840 })
        );
        assert_eq!(
            script_can_pump(u32::MAX),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x481840 })
        );
    }

    #[test]
    fn rva_ba300_sebrali_mi_kaaaru_svine_reports_unsupported() {
        // C_traffic_car::SebraliMiKaaaruSvine GAME RVA 0xba300 (VA 0x4ba300).
        assert_eq!(SEBRALI_MI_KAARU_SVINE_ENTRY, 0x4ba300);
        assert_eq!(
            sebrali_mi_kaaaru_svine(0x1234),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x4ba300 })
        );
    }

    #[test]
    fn rva_1b69d0_load_road_blocks_reset_empties() {
        // C_game::LoadRoadBlocks GAME RVA 0x1b69d0 (VA 0x5b69d0): reset half.
        let mut blocks = vec![
            RoadBlockEntry { node: 3, active: true },
            RoadBlockEntry { node: 9, active: false },
        ];
        load_road_blocks_reset(&mut blocks);
        assert!(blocks.is_empty());
        load_road_blocks_reset(&mut blocks);
        assert!(blocks.is_empty());
    }

    #[test]
    fn rva_1b6a50_collect_road_block_admits_only_valid_node() {
        // C_game::S_hlp::cbRoadBlock GAME RVA 0x1b6a50 (VA 0x5b6a50).
        let mut out = vec![];
        assert!(collect_road_block(0x1234, &mut out));
        assert!(!collect_road_block(0xffff, &mut out));
        assert_eq!(out, vec![RoadBlockEntry { node: 0x1234, active: false }]);
        assert!(collect_road_block(0, &mut out));
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn rva_1b6b10_load_taxi_points_reset_empties() {
        // C_game::LoadTaxiPoints GAME RVA 0x1b6b10 (VA 0x5b6b10): reset half.
        let mut points = vec![1, 2, 3];
        load_taxi_points_reset(&mut points);
        assert!(points.is_empty());
    }

    #[test]
    fn rva_1b90b0_load_scream_areas_reset_empties() {
        // C_game::LoadScreamAreas GAME RVA 0x1b90b0 (VA 0x5b90b0): reset half.
        let mut areas = vec![ScreamAreaEntry { frame: 1, id: 7 }];
        load_scream_areas_reset(&mut areas);
        assert!(areas.is_empty());
    }

    #[test]
    fn rva_1b7860_collect_signal_mark_indices_scans_backwards() {
        // C_game::CreateSignalMarkIDX GAME RVA 0x1b7860 (VA 0x5b7860).
        assert!(signal_mark_model_matches("DOPRAVNI1b.i3d"));
        assert!(signal_mark_model_matches("dopravni8B.I3D"));
        assert!(!signal_mark_model_matches("dopravni9B.I3D"));
        assert!(!signal_mark_model_matches("alarm01.i3d"));
        let sets = ["other.i3d", "dopravni3B.I3D", "x.i3d", "DOPRAVNI1B.I3D"];
        assert_eq!(collect_signal_mark_indices(&sets), vec![3, 1]);
        let empty: [&str; 0] = [];
        assert!(collect_signal_mark_indices(&empty).is_empty());
    }

    struct HydrantLog {
        destroyed: Vec<i32>,
        released: Vec<FrameId>,
    }

    impl HydrantBackend for HydrantLog {
        fn destroy_particle(&mut self, particle: i32) {
            self.destroyed.push(particle);
        }

        fn release_frame(&mut self, frame: FrameId) {
            self.released.push(frame);
        }
    }

    #[test]
    fn rva_1b9ef0_del_all_hydrants_runs_back_to_front_then_clears() {
        // C_game::DelAllHydrants GAME RVA 0x1b9ef0 (VA 0x5b9ef0).
        let mut hydrants = vec![
            HydrantEntry { particle: 11, frame: Some(1) },
            HydrantEntry { particle: 22, frame: None },
            HydrantEntry { particle: 33, frame: Some(3) },
        ];
        let mut backend = HydrantLog { destroyed: vec![], released: vec![] };
        del_all_hydrants(&mut hydrants, &mut backend);
        assert!(hydrants.is_empty());
        assert_eq!(backend.destroyed, vec![33, 22, 11]);
        assert_eq!(backend.released, vec![3, 1]);
    }

    #[test]
    fn rva_1b6d10_is_valid_taxi_passenger_returns_false() {
        // C_game::IsValidTaxiPassenger GAME RVA 0x1b6d10 (VA 0x5b6d10): STUB.
        assert!(!is_valid_taxi_passenger());
    }

    #[test]
    fn rva_1b7560_add_human_talk_reports_unsupported() {
        // C_game::AddHumanTalk GAME RVA 0x1b7560 (VA 0x5b7560): STUB.
        assert_eq!(ADD_HUMAN_TALK_ENTRY, 0x5b7560);
        assert_eq!(
            add_human_talk(4, 2),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x5b7560 })
        );
    }

    #[test]
    fn rva_1b7730_del_human_talk_reports_unsupported() {
        // C_game::DelHumanTalk GAME RVA 0x1b7730 (VA 0x5b7730): STUB.
        assert_eq!(DEL_HUMAN_TALK_ENTRY, 0x5b7730);
        assert_eq!(
            del_human_talk(4, 2),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x5b7730 })
        );
    }

    #[test]
    fn rva_1b77c0_death_human_talk_reports_unsupported() {
        // C_game::DeathHumanTalk GAME RVA 0x1b77c0 (VA 0x5b77c0): STUB.
        assert_eq!(DEATH_HUMAN_TALK_ENTRY, 0x5b77c0);
        assert_eq!(
            death_human_talk(9),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x5b77c0 })
        );
    }
}
