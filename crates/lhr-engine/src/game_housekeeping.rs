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

/// FUNCTION: GAME RVA 0x1b6b60, VA 0x5b6b60 (`game.cpp:3627-3632`,
/// `C_game::S_hlp::cbFindTaxiPoints`).
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

impl ScoreState {
    /// FUNCTION: GAME 0x47b6b0 (RVA 0x7b6b0, VA 0x47b6b0; `game.h:952-956`,
    /// `C_game::GetScore`): reads the stored score.
    pub fn get_score(&self) -> i32 {
        self.score
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

/// Pure model-set fragment of `C_game::LoadTelephones` (part of the occupied
/// GAME 0x5b6e00 body; sibling `game.cpp:3649-3660`, cited only): scan the
/// base-cache model-set names from the back for `alarm01.i3d` and
/// `PHbox01.i3d` (ASCII-case-insensitive, the native `_stricmp`), returning
/// the kept indices. Each plain assignment while scanning backwards keeps
/// the lowest matching index; `None` is the native `0xffff` (no match).
/// The scene enumeration (`EnumFrames`) and the block expansion below stay
/// with the scene/cache owners (DEFERRED). No function credit is taken.
pub fn telephone_model_indices(model_sets: &[impl AsRef<str>]) -> (Option<u16>, Option<u16>) {
    let mut alarm: Option<u16> = None;
    let mut box_: Option<u16> = None;
    for (index, name) in model_sets.iter().enumerate().rev() {
        let name = name.as_ref();
        let base = name.rsplit('\\').next().unwrap_or(name);
        if base.eq_ignore_ascii_case("alarm01.i3d") {
            alarm = Some(index as u16);
        }
        if base.eq_ignore_ascii_case("PHbox01.i3d") {
            box_ = Some(index as u16);
        }
    }
    (alarm, box_)
}

/// Null-frame marker for [`collect_block_telephones`]: stands in for the
/// native NULL `frame` of base-cache telephone records (the scene owner
/// links the real cache model later). Scene-enumerated records always carry
/// real nonzero handles.
pub const TELEPHONE_NULL_FRAME: FrameId = 0;

/// Pure block fragment of `C_game::LoadTelephones` (part of the occupied
/// GAME 0x5b6e00 body; sibling `game.cpp:3661-3682`, cited only): expand the
/// base-cache blocks from the back, each block's models from the back. A
/// model matching the alarm index pushes a non-phone-box record, one
/// matching the box index a phone-box record (the alarm arm wins ties, the
/// native `if`/`else if`); other models push nothing. All pushed records
/// carry [`TELEPHONE_NULL_FRAME`]. The caller appends the run to the
/// collected telephones. No function credit is taken.
pub fn collect_block_telephones(
    alarm: Option<u16>,
    box_: Option<u16>,
    blocks: &[Vec<u16>],
) -> Vec<TelephoneEntry> {
    let mut out = Vec::new();
    for models in blocks.iter().rev() {
        for &model in models.iter().rev() {
            if Some(model) == alarm {
                out.push(TelephoneEntry {
                    frame: TELEPHONE_NULL_FRAME,
                    phone_box: false,
                });
            } else if Some(model) == box_ {
                out.push(TelephoneEntry {
                    frame: TELEPHONE_NULL_FRAME,
                    phone_box: true,
                });
            }
        }
    }
    out
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

/// FUNCTION: GAME 0x5add70 (RVA 0x1add70, VA 0x5add70; `game.cpp:2959-2973`,
/// `C_game::FreeSemaphores`): switches every collected lamp back on from the
/// back, then empties the list. Absent lamps (`None`) are skipped; the scene
/// enumeration that collected the entries stays with the scene owner.
pub fn free_semaphores(
    semaphores: &mut Vec<SemaphoreEntry>,
    backend: &mut impl SemaphoreLightBackend,
) {
    for index in (0..semaphores.len()).rev() {
        let entry = semaphores[index];
        if let Some(frame) = entry.red {
            backend.set_light(frame, true);
        }
        if let Some(frame) = entry.orange {
            backend.set_light(frame, true);
        }
        if let Some(frame) = entry.green {
            backend.set_light(frame, true);
        }
    }
    semaphores.clear();
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
    /// Raw cheat word read by [`GameInlineState::get_cheats_bit6`]
    /// (`game.h:834-840` at pin 113b5b6). No cheat word toggles bit 6; the
    /// writer stays with the cheat owner (DEFERRED).
    cheats: u32,
    /// Staged identities for the object accessors below. The native bodies
    /// return references owned elsewhere (`G_Camera`, `C_using_object`,
    /// `C_sledovacka`, `soundArena`, `C_shvestky`); the host stages the
    /// caller-provided handle and the getters read it back. `None` is the
    /// native NULL.
    camera: Option<u32>,
    using_object: Option<u32>,
    sledovacka: Option<u32>,
    sound_arena: Option<u32>,
    shvestky: Option<u32>,
    actor_lightness: bool,
    anim_preload: bool,
    weather_buffer: i32,
    /// Night-mission switch (`game.h:419`, offset 0x2d44, `GAME_NIGHTMISSION's`).
    /// Written by [`GameInlineState::set_night_mission`].
    night_mission: bool,
    /// Script float-register handle for `RACING_MISSION6_START` (`game.h:459`,
    /// offset 0x2ee0). `None` is the native NULL; the register itself stays
    /// with the script owner. Written by [`GameInlineState::set_race6_result`].
    race6_result: Option<u32>,
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

    /// FUNCTION: GAME 0x47b510 (RVA 0x7b510, VA 0x47b510; `game.h:842-846`,
    /// `C_game::GetCamera`): reads the staged camera handle. The native body
    /// returns the owned `G_Camera` reference; the object stays with its
    /// owner (DEFERRED) and `None` is the native NULL.
    pub fn get_camera(&self) -> Option<u32> {
        self.camera
    }

    /// Staging for [`GameInlineState::get_camera`]; the camera owner itself
    /// stays with the caller.
    pub fn stage_camera(&mut self, camera: Option<u32>) {
        self.camera = camera;
    }

    /// FUNCTION: GAME 0x47b530 (RVA 0x7b530, VA 0x47b530; `game.h:852-856`,
    /// `C_game::GetUsingObject`): reads the staged using-object handle. The
    /// native body returns the owned `C_using_object` reference; the object
    /// stays with its owner (DEFERRED).
    pub fn get_using_object(&self) -> Option<u32> {
        self.using_object
    }

    /// Staging for [`GameInlineState::get_using_object`].
    pub fn stage_using_object(&mut self, using_object: Option<u32>) {
        self.using_object = using_object;
    }

    /// FUNCTION: GAME 0x47b550 (RVA 0x7b550, VA 0x47b550; `game.h:863-867`,
    /// `C_game::GetSledovacka`): reads the staged sledovacka handle. The
    /// native body returns the owned `C_sledovacka` reference; the object
    /// stays with its owner (DEFERRED).
    pub fn get_sledovacka(&self) -> Option<u32> {
        self.sledovacka
    }

    /// Staging for [`GameInlineState::get_sledovacka`].
    pub fn stage_sledovacka(&mut self, sledovacka: Option<u32>) {
        self.sledovacka = sledovacka;
    }

    /// FUNCTION: GAME 0x47b580 (RVA 0x7b580, VA 0x47b580; `game.h:878-882`,
    /// `C_game::GetSoundArena`): reads the staged sound-arena handle. The
    /// native body returns the owned `soundArena` reference; the object
    /// stays with its owner (DEFERRED).
    pub fn get_sound_arena(&self) -> Option<u32> {
        self.sound_arena
    }

    /// Staging for [`GameInlineState::get_sound_arena`].
    pub fn stage_sound_arena(&mut self, sound_arena: Option<u32>) {
        self.sound_arena = sound_arena;
    }

    /// FUNCTION: GAME 0x47b620 (RVA 0x7b620, VA 0x47b620; `game.h:913-916`,
    /// `C_game::GetShvestky`): reads the staged shvestky (police) handle.
    /// The native body returns the owned `C_shvestky` reference; the object
    /// stays with its owner (DEFERRED).
    pub fn get_shvestky(&self) -> Option<u32> {
        self.shvestky
    }

    /// Staging for [`GameInlineState::get_shvestky`].
    pub fn stage_shvestky(&mut self, shvestky: Option<u32>) {
        self.shvestky = shvestky;
    }

    /// FUNCTION: GAME 0x47b680 (RVA 0x7b680, VA 0x47b680; `game.h:937-941`,
    /// `C_game::SetActorLightness`): stores the actor-lightness switch.
    pub fn set_actor_lightness(&mut self, on: bool) {
        self.actor_lightness = on;
    }

    /// Reads the switch written by [`GameInlineState::set_actor_lightness`].
    pub fn actor_lightness(&self) -> bool {
        self.actor_lightness
    }

    /// FUNCTION: GAME 0x47b690 (RVA 0x7b690, VA 0x47b690; `game.h:942-946`,
    /// `C_game::SetAnimPreload`): stores the animation-preload switch.
    pub fn set_anim_preload(&mut self, on: bool) {
        self.anim_preload = on;
    }

    /// Reads the switch written by [`GameInlineState::set_anim_preload`].
    pub fn anim_preload(&self) -> bool {
        self.anim_preload
    }

    /// FUNCTION: GAME 0x47b6a0 (RVA 0x7b6a0, VA 0x47b6a0; `game.h:947-951`,
    /// `C_game::SetWeatherBuffer`): stores the weather buffer value.
    pub fn set_weather_buffer(&mut self, value: i32) {
        self.weather_buffer = value;
    }

    /// Reads the value written by [`GameInlineState::set_weather_buffer`].
    pub fn weather_buffer(&self) -> i32 {
        self.weather_buffer
    }

    /// Staging for [`GameInlineState::get_cheats_bit6`]; the cheat-word
    /// writer itself stays with the cheat owner.
    pub fn set_cheats(&mut self, cheats: u32) {
        self.cheats = cheats;
    }

    /// Inline cheat-bit read (working name; `game.h:834-840` at pin 113b5b6,
    /// `C_game::GetCheatsBit6`): bit 6 of the cheat word, with which
    /// `C_shvestky::Tick` does nothing. The `GetCheatsBit6` spelling is an
    /// upstream working guess, disclosed here; the read itself is inline, so
    /// no outline RVA is claimed. The police tick stays with its owner
    /// (DEFERRED).
    pub fn get_cheats_bit6(&self) -> bool {
        (self.cheats >> 6) & 1 != 0
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

impl GameInlineState {
    /// FUNCTION: GAME RVA 0x7b590, VA 0x47b590 (`game.h:883-887`,
    /// `C_game::SetNightMission`): stores the night-mission switch.
    pub fn set_night_mission(&mut self, night: bool) {
        self.night_mission = night;
    }

    /// Reads the switch stored by [`GameInlineState::set_night_mission`].
    pub fn night_mission(&self) -> bool {
        self.night_mission
    }

    /// FUNCTION: GAME RVA 0x7b670, VA 0x47b670 (`game.h:932-936`,
    /// `C_game::SetRace6Result`): stores the script float-register handle.
    pub fn set_race6_result(&mut self, result: Option<u32>) {
        self.race6_result = result;
    }

    /// Reads the handle stored by [`GameInlineState::set_race6_result`].
    pub fn race6_result(&self) -> Option<u32> {
        self.race6_result
    }
}

/// FUNCTION: GAME RVA 0x147a60, VA 0x547a60 (`game.h:584-590`, `~C_game`):
/// empties the inventory, character and float stacks through the three stack
/// clears, in native member order. Element types stay with their owners.
pub fn game_drop_stacks(
    inventory: &mut Vec<InventorySnapshot>,
    character: &mut Vec<u32>,
    float_stack: &mut Vec<f32>,
) {
    inventory_stack_clear(inventory);
    character_stack_clear(character);
    float_stack_clear(float_stack);
}

/// Native `C_actor::SetActState(ACTSTATE_NONE)` sink for
/// [`remove_temporary_actor`].
pub trait RemovedActorBackend {
    fn set_act_state_none(&mut self, actor: ActorId);
}

/// FUNCTION: GAME RVA 0x1a79a0, VA 0x5a79a0 (`game.cpp:2422-2432`,
/// `C_game::RemoveTemporaryActor`): no-op when the actor is already listed as
/// removed; otherwise the actor leaves the active set through the backend and
/// joins the removed list. The caller stages the removed list; act-state
/// ownership stays with the actor owner behind the backend.
pub fn remove_temporary_actor(
    removed: &mut Vec<ActorId>,
    actor: ActorId,
    backend: &mut impl RemovedActorBackend,
) {
    if removed.contains(&actor) {
        return;
    }
    backend.set_act_state_none(actor);
    removed.push(actor);
}

/// Native per-actor sink for [`clear_temporary_actors`]: `InvalidateActor`
/// (with `false`), `GameDone` and `Release`, in that order per actor from the
/// back of the list.
pub trait TemporaryActorBackend {
    fn invalidate_actor(&mut self, actor: ActorId);
    fn actor_game_done(&mut self, actor: ActorId);
    fn release_actor(&mut self, actor: ActorId);
}

/// FUNCTION: GAME RVA 0x1a7f60, VA 0x5a7f60 (`game.cpp:2484-2497`,
/// `C_game::ClearTemporaryActors`): every temporary actor but the next
/// mission's two (`NewMissionActors[0..2]`, staged here as `keep`) is
/// invalidated, ended and released from the back, then the list is emptied.
/// Frame invalidation, mission end and release stay with their owners behind
/// the backend.
pub fn clear_temporary_actors(
    temporary: &mut Vec<ActorId>,
    keep: [Option<ActorId>; 2],
    backend: &mut impl TemporaryActorBackend,
) {
    for index in (0..temporary.len()).rev() {
        let actor = temporary[index];
        if keep.contains(&Some(actor)) {
            continue;
        }
        backend.invalidate_actor(actor);
        backend.actor_game_done(actor);
        backend.release_actor(actor);
    }
    temporary.clear();
}

/// STUB: GAME RVA 0x1acfe0, VA 0x5acfe0 (`game.cpp:2874-2888`,
/// `C_game::AddShadowModel`): creates a `FRAME_SHADOW` frame through the
/// render driver, opens the named shadow model and pushes the basename copy.
/// The driver and frame owners are DEFERRED; explicit unsupported boundary.
pub const ADD_SHADOW_MODEL_ENTRY: u32 = 0x5acfe0;

/// Original `C_game::AddShadowModel` stub: always reports the boundary
/// instead of inventing driver/frame ownership.
pub fn add_shadow_model(_name: &str) -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: ADD_SHADOW_MODEL_ENTRY,
    })
}

/// Shadow-model template entry (`S_shadow_temp`): only the compared `name`
/// (`None` is the native NULL) participates here; the `I3D_shadow` model stays
/// with the scene owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShadowTemplateEntry {
    pub name: Option<String>,
}

/// Selected duplicate source for [`setup_shadow_select`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShadowDuplicateSource {
    /// Duplicate `templates[index]` (basename match, scanned from the back).
    Template(usize),
    /// Duplicate `templates[model_index]` (nameless item fallback).
    ModelIndex(usize),
    /// Named item with no matching template: the original returns early and
    /// duplicates nothing.
    NoMatch,
}

/// FUNCTION: GAME RVA 0x1ad210, VA 0x5ad210 (`game.cpp:2890-2907`,
/// `C_game::SetupShadow`): pure source-selection half. A named item scans the
/// templates from the back for an ASCII-case-insensitive (`_stricmp`) match;
/// a nameless item falls back to `model_index`; a named item with no match
/// duplicates nothing. The `I3D_shadow::Duplicate` call stays with the scene
/// owner (DEFERRED).
pub fn setup_shadow_select(
    templates: &[ShadowTemplateEntry],
    name: Option<&str>,
    model_index: usize,
) -> ShadowDuplicateSource {
    match name {
        Some(wanted) => {
            for (index, template) in templates.iter().enumerate().rev() {
                if let Some(candidate) = template.name.as_deref() {
                    if candidate.eq_ignore_ascii_case(wanted) {
                        return ShadowDuplicateSource::Template(index);
                    }
                }
            }
            ShadowDuplicateSource::NoMatch
        }
        None => ShadowDuplicateSource::ModelIndex(model_index),
    }
}

/// Actor candidate for [`entity_block`]: only the `type == ACTOR_ENTITY`
/// filter participates here; the entity behavior stays with the actor owner
/// behind the backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockCandidate {
    pub actor: ActorId,
    pub is_entity: bool,
}

/// Native `C_entity` sink for [`entity_block`]: seating for the cinema versus
/// waking for the subtitles.
pub trait EntityBlockBackend {
    fn seat_for_cinema(&mut self, actor: ActorId, unknown: bool);
    fn wake_for_subtitles(&mut self, actor: ActorId);
}

/// FUNCTION: GAME RVA 0x1b7ed0, VA 0x5b7ed0 (`game.cpp:3778-3802`,
/// `C_game::EntityBlock`): every entity actor of the active list, then of the
/// inactive list, both scanned from the back, is seated for the cinema while
/// `block` holds (carrying `unknown`) and woken for the subtitles otherwise.
/// Entity behavior stays with the actor owner behind the backend.
pub fn entity_block(
    block: bool,
    unknown: bool,
    active: &[BlockCandidate],
    inactive: &[BlockCandidate],
    backend: &mut impl EntityBlockBackend,
) {
    for list in [active, inactive] {
        for candidate in list.iter().rev() {
            if !candidate.is_entity {
                continue;
            }
            if block {
                backend.seat_for_cinema(candidate.actor, unknown);
            } else {
                backend.wake_for_subtitles(candidate.actor);
            }
        }
    }
}

/// Native `C_actor::SoundsChange` sink for [`stop_all_sounds`].
pub trait ActorSoundBackend {
    fn sounds_change(&mut self, actor: ActorId, param: i32);
}

/// FUNCTION: GAME RVA 0x1b83f0, VA 0x5b83f0 (`game.cpp:3828-3834`,
/// `C_game::StopAllSounds`): every global actor, scanned from the back, gets
/// `SoundsChange(0)`. Actor/sound ownership stays behind the backend.
pub fn stop_all_sounds(actors: &[ActorId], backend: &mut impl ActorSoundBackend) {
    for index in (0..actors.len()).rev() {
        backend.sounds_change(actors[index], 0);
    }
}

/// Marker for one fading dead human (`C_game::S_death_human_fade`,
/// `game.h:549`). Only list membership participates here; the per-entry
/// `RemoveDeathHumanFade` (GAME 0x5baa40, unwritten upstream STUB) and the
/// fade tick stay with their owners.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeathHumanFadeEntry {
    pub marker: u32,
}

/// FUNCTION: GAME RVA 0x1ba310, VA 0x5ba310 (`game.cpp:3994-4002`,
/// `C_game::ClearDeathHumanFade`): the original runs `RemoveDeathHumanFade(i,
/// false)` per entry from the back; that callee is unwritten upstream (STUB
/// GAME 0x5baa40, a `StubLog` no-op), so the observable effect here is the
/// emptied list plus the cleared item-blur switch.
pub fn clear_death_human_fade(fades: &mut Vec<DeathHumanFadeEntry>, item_blur: &mut bool) {
    fades.clear();
    *item_blur = false;
}

/// Owned weather-switch slice read and written by [`osefuj_pocasi`]
/// (`game.h:509-510`: `weather_on`, `weather_count`; `game_effects` selects
/// the divisor).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WeatherSwitchState {
    pub initialized: bool,
    pub weather_on: bool,
    pub weather_count: i32,
    pub game_effects: i32,
}

/// Native scene weather-system sink for [`osefuj_pocasi`]:
/// `GetWeatherSystemParam` / `SetWeatherSystemParam`. The parameter ids
/// (`WS_PARAM_ON`, `WS_PARAM_COUNT`) live with the LS3DF scene owner; the
/// caller passes them in so no value is invented here.
pub trait WeatherSystemBackend {
    fn weather_param(&self, param: u32) -> i32;
    fn set_weather_param(&mut self, param: u32, value: i32);
}

/// FUNCTION: GAME RVA 0x1b9540, VA 0x5b9540 (`game.cpp:3896-3932`,
/// `C_game::OsefujPocasi`, "deal with the weather"): with `read`, copies the
/// scene's on/count switch into the stored pair (only the matching `param`)
/// and then applies the write pass; without `read` and while on, sets the
/// scene's on-switch and scales the count by the effects level (full, /8,
/// /20; other levels leave the scene untouched). Integer division truncates
/// toward zero as in the original. The scene weather system stays behind the
/// backend.
pub fn osefuj_pocasi(
    read: bool,
    param: u32,
    param_on: u32,
    param_count: u32,
    state: &mut WeatherSwitchState,
    backend: &mut impl WeatherSystemBackend,
) {
    if !state.initialized {
        return;
    }
    if read {
        if param == param_on {
            state.weather_on = backend.weather_param(param_on) != 0;
        } else if param == param_count {
            state.weather_count = backend.weather_param(param_count);
        }
    }
    if !state.weather_on {
        return;
    }
    match state.game_effects {
        0 => {
            backend.set_weather_param(param_on, 1);
            backend.set_weather_param(param_count, state.weather_count / 20);
        }
        1 => {
            backend.set_weather_param(param_on, 1);
            backend.set_weather_param(param_count, state.weather_count / 8);
        }
        2 => {
            backend.set_weather_param(param_on, 1);
            backend.set_weather_param(param_count, state.weather_count);
        }
        _ => {}
    }
}

/// Owned inter-city collision boxes (`game.h:550-556`): the collision array
/// (`None` is the native NULL), the frame array, the two plain `S_matrix`
/// arrays (presence only; the original deletes them as a whole with no
/// per-element work) and the arrays' count.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InterCityBoxes {
    pub colls: Option<Vec<Option<u32>>>,
    pub frames: Option<Vec<Option<FrameId>>>,
    pub matrices_present: bool,
    pub inv_matrices_present: bool,
    pub num: usize,
}

/// Native collision/frame sink for [`inter_city_game_done`]: `DynErase` per
/// present collision and `Release` per present frame. Collision and frame
/// ownership stays behind the backend.
pub trait InterCityBackend {
    fn erase_coll(&mut self, coll: u32);
    fn release_frame(&mut self, frame: FrameId);
}

/// FUNCTION: GAME RVA 0x1bbb20, VA 0x5bbb20 (`game.cpp:4084-4112`,
/// `C_game::InterCity_GameDoneInGameDone`): with no collision array, a no-op;
/// otherwise every present collision of the first `num` slots, scanned from
/// the back, is erased, every present frame released, the count cleared and
/// all four arrays dropped. The scan is bounded by the shortest staged array
/// (host accommodation; the original trusts the sizes to agree).
pub fn inter_city_game_done(boxes: &mut InterCityBoxes, backend: &mut impl InterCityBackend) {
    let Some(colls) = boxes.colls.as_mut() else {
        return;
    };
    let Some(frames) = boxes.frames.as_mut() else {
        return;
    };
    let count = boxes.num.min(colls.len()).min(frames.len());
    for index in (0..count).rev() {
        if let Some(coll) = colls[index] {
            backend.erase_coll(coll);
        }
        if let Some(frame) = frames[index] {
            backend.release_frame(frame);
        }
    }
    boxes.num = 0;
    boxes.colls = None;
    boxes.frames = None;
    boxes.matrices_present = false;
    boxes.inv_matrices_present = false;
}

/// Fragment of GAME RVA 0x1bab80, VA 0x5bab80 (`game.cpp:4024-4031, 4060-4070`,
/// `C_game::InterCity_GameInit`; no own entry): the box-count decision. The
/// base blocks closed around the city get nine collision boxes, and four more
/// in the `mise16-mesto` mission (ASCII-case-insensitive `_stricmp`). Array
/// allocation, box poses and collision/frame registration stay with the
/// collision and scene owners (DEFERRED).
pub fn inter_city_box_count(mission_name: &str) -> usize {
    if mission_name.eq_ignore_ascii_case("mise16-mesto") {
        13
    } else {
        9
    }
}

/// STUB: GAME RVA 0x1ae480, VA 0x5ae480 (`game.cpp:3056-3061`,
/// `C_game::LoadDiffData`): loads the mission's diff data by name. Unwritten
/// upstream; the mission save/load owner is DEFERRED. Explicit unsupported
/// boundary.
pub const LOAD_DIFF_DATA_ENTRY: u32 = 0x5ae480;

/// Original `C_game::LoadDiffData` stub: always reports the boundary instead
/// of inventing diff-data loading.
pub fn load_diff_data(_name: &str) -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: LOAD_DIFF_DATA_ENTRY,
    })
}

/// Intern-load handshake flags (`game.h:520-521`, `intern_load` /
/// `intern_loaded`).
///
/// Working name; no outline GAME entry is claimed. Reason: `ResetIntern`
/// (`game.h:829-833` at pin 113b5b6) is inline-only, so there is no
/// out-of-line RVA to pin. The mission opener and the `Init`/`DoneIntern`
/// swap stay with their owners; only the two stored flags are owned here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InternLoadFlags {
    pub intern_load: bool,
    pub intern_loaded: bool,
}

/// Working-name fragment of `C_game::ResetIntern` (`game.h:829-833`):
/// `intern_load = true`, `intern_loaded = false`. No function credit is
/// taken; the inline has no outline GAME entry.
pub fn reset_intern_flags(flags: &mut InternLoadFlags) {
    flags.intern_load = true;
    flags.intern_loaded = false;
}

/// Working-name fragment of `C_game::Init` (`game.cpp:269-271` at pin
/// 113b5b6): `score_on = false`, the `ScoreSetOn(false)` HUD effect, then
/// `score = 0` with no further HUD write because the switch is off. No
/// outline GAME entry is claimed for this fragment; no function credit is
/// taken. The occupied `Init` entry (GAME 0x5a0810, RVA 0x1a0810) keeps its
/// existing port.
pub fn game_init_score_reset(state: &mut ScoreState, hud: &mut impl HudBonusBackend) {
    score_set_on(state, false, hud);
    state.score = 0;
}

/// Tick counters cleared by `C_game::Init` (`game.h:411-415`,
/// `game_time` / `game_frames` / `init_frames`).
///
/// Working name; no outline GAME entry is claimed. Reason: pure fragment of
/// the occupied `Init` body (`game.cpp:297-299`), which keeps its existing
/// port. No function credit is taken.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GameTimeCounters {
    pub game_time: i32,
    pub game_frames: i32,
    pub init_frames: u32,
}

/// Working-name fragment of `C_game::Init` (`game.cpp:297-299`):
/// `game_time = 0`, `game_frames = 0`, `init_frames = 0`. No function credit
/// is taken.
pub fn game_init_time_clear(counters: &mut GameTimeCounters) {
    counters.game_time = 0;
    counters.game_frames = 0;
    counters.init_frames = 0;
}

/// End-of-mission flags cleared by `C_game::Init` (`game.h:304`,
/// `mission_end`; `game.h:455`, `game_done`; `game.h:390`,
/// `growing_projectors`).
///
/// Working name; no outline GAME entry is claimed. Reason: pure fragment of
/// the occupied `Init` body (`game.cpp:294-296`). No function credit is
/// taken.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MissionEndFlags {
    pub game_done: bool,
    pub mission_end: i32,
    pub growing_projectors: i32,
}

/// Working-name fragment of `C_game::Init` (`game.cpp:294-296`):
/// `game_done = false`, `mission_end = 0`, `growing_projectors = 0`. No
/// function credit is taken.
pub fn game_init_mission_end_clear(flags: &mut MissionEndFlags) {
    flags.game_done = false;
    flags.mission_end = 0;
    flags.growing_projectors = 0;
}

/// Working-name fragment of `C_game::Init` (`game.cpp:273`):
/// `heal_cheat = true`. No outline GAME entry is claimed for this single
/// scalar store; no function credit is taken.
pub fn game_init_heal_cheat() -> bool {
    true
}

/// `C_game::Init` death-fade timeout (`game.cpp:275` at pin 113b5b6,
/// `death_fade_time = 7000`, `game.h:501`). Working-name constant; the
/// store itself has no outline GAME entry, so no RVA is claimed.
pub const GAME_INIT_DEATH_FADE_TIME: i32 = 7000;

/// Working-name fragment of `C_game::Init` (`game.cpp:275`): returns the
/// `death_fade_time` value `Init` stores. No function credit is taken.
pub fn game_init_death_fade_time() -> i32 {
    GAME_INIT_DEATH_FADE_TIME
}

/// Cheat input state cleared by `C_game::Init` (`game.h:467-468`:
/// `cheat_keys[0x20]` / `cheats`).
///
/// Working name; no outline GAME entry is claimed. Reason: pure fragment of
/// the occupied `Init` body (`game.cpp:292-293`). No function credit is
/// taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheatKeysState {
    pub keys: [u8; 32],
    pub cheats: u32,
}

impl Default for CheatKeysState {
    fn default() -> Self {
        Self {
            keys: [0; 32],
            cheats: 0,
        }
    }
}

/// Working-name fragment of `C_game::Init` (`game.cpp:292-293`):
/// `strcpy(cheat_keys, "")` then `cheats = 0`. `strcpy` only guarantees
/// `keys[0] == 0`, so the reset stores `0` at index 0 and leaves the
/// remaining bytes untouched, exactly like the original. No function credit
/// is taken.
pub fn game_init_cheats_reset(state: &mut CheatKeysState) {
    state.keys[0] = 0;
    state.cheats = 0;
}

/// Working-name fragment of `C_game::Init` (`game.cpp:300-305`):
/// `race6_state = RACE6_STATE_NONE` and the four timeout/player script
/// programs back to NULL, leaving the paired event words untouched (the
/// original stores only the program pointers here). No outline GAME entry
/// is claimed for this fragment; no function credit is taken.
pub fn reset_race6_and_timeout_events(state: &mut GameInlineState) {
    let (_, timeout_event) = state.timeout_event();
    state.set_timeout_event(0, timeout_event);
    let (_, fire_event) = state.player_fire_event();
    state.set_player_fire_event(0, fire_event);
    let (_, horn_event) = state.player_horn_event();
    state.set_player_horn_event(0, horn_event);
    let (_, wfall_event) = state.player_wfall_event();
    state.set_player_wfall_event(0, wfall_event);
    state.set_race6_state(false);
}

/// Working-name fragment of `C_game::Init` (`game.cpp:259`):
/// `weather_buffer = -1`. The scene-driven `weather_on` / `weather_count`
/// reads stay with the scene owner; only the buffer staging through the
/// existing [`GameInlineState::set_weather_buffer`] is owned here. No
/// outline GAME entry is claimed; no function credit is taken.
pub fn game_init_weather_buffer_reset(state: &mut GameInlineState) {
    state.set_weather_buffer(-1);
}

/// Working-name fragment of `C_game::Init` (`game.cpp:306`):
/// `death_actors.clear()`. The actor owner stays with the caller, which
/// passes the staged `uint` list. No outline GAME entry is claimed for this
/// fragment; no function credit is taken.
pub fn clear_death_actors(actors: &mut Vec<u32>) {
    actors.clear();
}

/// Working-name fragment of `C_game::Init` (`game.cpp:307-308`):
/// `light_cache.clear()` with `light_cache_time = -1`. Frames stay opaque
/// handles for the scene owner. No outline GAME entry is claimed for this
/// fragment; no function credit is taken.
pub fn clear_light_cache(cache: &mut Vec<FrameId>, cache_time: &mut i32) {
    cache.clear();
    *cache_time = -1;
}

/// FUNCTION: GAME RVA 0x178c60, VA 0x578c60 (`game/actors/C_human.cpp:4492-4533`
/// at pin 113b5b6, `C_human::IsPossibleColl`): whether the actor at `pos`
/// moving by `dir` runs into this human.
///
/// The human is a sphere of `radius + 0.7` around the chest
/// (`human_pos.y + 1.0`) swept by a tenth of `human_dir`; the function
/// reports the first-meet fraction in `dist` (`0` where they already meet)
/// and the human as the collider. The unused `C_actor *act` parameter of the
/// original has no effect and is not modeled. The `car` early-out (non-NULL
/// car refuses the collision without touching `dist`) arrives as `in_car`;
/// with `in_car` the function returns `None` meaning "return false, leave
/// `dist` unwritten". Otherwise it returns `Some((dist, hit))` where `hit`
/// is the original `t >= 0.0f` return.
///
/// Verified in reference: `game/actors/C_human.cpp` belongs to Game.exe and
/// the marker is `// FUNCTION: GAME 0x578c60` (code, not a data offset and
/// not another binary). RVA `0x178c60` = `0x578c60 - 0x400000`; VA
/// `0x400000 + 0x178c60` = `0x578c60`. The RVA appears in neither
/// `docs/rewrite-overlay.json` nor `docs/progress.json`. No original-runtime
/// validation is claimed.
pub fn is_possible_coll(
    in_car: bool,
    human_dir: [f32; 3],
    human_pos: [f32; 3],
    pos: [f32; 3],
    dir: [f32; 3],
    radius: f32,
) -> Option<(f32, bool)> {
    if in_car {
        return None;
    }
    let step = [
        human_dir[0] * 0.1,
        human_dir[1] * 0.1,
        human_dir[2] * 0.1,
    ];
    let center = [human_pos[0], human_pos[1] + 1.0, human_pos[2]];
    let a = [center[0] - pos[0], center[1] - pos[1], center[2] - pos[2]];
    let b = [step[0] - dir[0], step[1] - dir[1], step[2] - dir[2]];
    let r = radius + 0.7;
    let c = a[0] * a[0] + a[1] * a[1] + a[2] * a[2];
    let r2 = r * r;
    let t = if c <= r2 {
        0.0
    } else {
        let aa = b[0] * b[0] + b[1] * b[1] + b[2] * b[2];
        if aa <= 0.0 {
            -1.0
        } else {
            let bb = (b[0] * a[0] + b[1] * a[1] + b[2] * a[2]) * 2.0;
            let disc = bb * bb - (c - r2) * aa * 4.0;
            if disc < 0.0 {
                disc
            } else {
                // The original calls the double-precision CRT sqrt before
                // narrowing to the `float s` local; avoid changing that
                // conversion boundary into a host single-precision intrinsic.
                let s = f64::from(disc).sqrt() as f32;
                let inv = 1.0 / (aa * 2.0);
                let t1 = (s - bb) * inv;
                let mut t = (-bb - s) * inv;
                if t1 < t {
                    t = t1;
                }
                t
            }
        }
    };
    Some((t, t >= 0.0))
}

/// STUB: GAME RVA 0x16b180, VA 0x56b180 (`game/actors/C_human.cpp:42-48` at
/// pin 113b5b6, `C_human::AddToCutscene`): records the human in the cutscene
/// with record sizes `0x28, 0x2c, 0x38` and returns `true`. The cutscene
/// owner is DEFERRED, so the host reports the boundary instead of inventing
/// cutscene ownership. Explicit unsupported boundary.
///
/// Verified in reference: `game/actors/C_human.cpp` belongs to Game.exe and
/// the marker is `// FUNCTION: GAME 0x56b180` (code, not a data offset and
/// not another binary). RVA `0x16b180` = `0x56b180 - 0x400000`; VA
/// `0x400000 + 0x16b180` = `0x56b180`. The RVA appears in neither
/// `docs/rewrite-overlay.json` nor `docs/progress.json`.
pub const ADD_TO_CUTSCENE_ENTRY: u32 = 0x56b180;

/// Original `C_human::AddToCutscene` stub: always reports the boundary
/// instead of inventing the cutscene owner.
pub fn add_to_cutscene() -> Result<bool, UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: ADD_TO_CUTSCENE_ENTRY,
    })
}

/// STUB: GAME RVA 0x1b9390, VA 0x5b9390 (`game.cpp:3887-3891`,
/// `game.h:703`, `C_game::TickScreamAreas`): unwritten upstream (`StubLog`
/// only). The camera-area id accumulation (`camera_scream_areas`,
/// `scream_areas_time`) stays with the scene owner (DEFERRED). Explicit
/// unsupported boundary.
///
/// Verified in reference: `game/mission/game.cpp` belongs to Game.exe and
/// the marker is `// STUB: GAME 0x5b9390` (code, not a data offset and not
/// another binary). RVA `0x1b9390` = `0x5b9390 - 0x400000`; VA
/// `0x400000 + 0x1b9390` = `0x5b9390`. The RVA appears in neither
/// `docs/rewrite-overlay.json` nor `docs/progress.json`.
pub const TICK_SCREAM_AREAS_ENTRY: u32 = 0x5b9390;

/// Original `C_game::TickScreamAreas` stub: always reports the boundary
/// instead of inventing the scene tick.
pub fn tick_scream_areas() -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: TICK_SCREAM_AREAS_ENTRY,
    })
}

/// STUB: GAME RVA 0x1baa40, VA 0x5baa40 (`game.cpp:4010-4014`,
/// `game.h:446`, `C_game::RemoveDeathHumanFade`): unwritten upstream
/// (`StubLog` only). The per-entry fade owner stays with its owner
/// (DEFERRED); `ClearDeathHumanFade` (GAME 0x5ba310, already ported) only
/// observes the emptied list plus the cleared blur switch. Explicit
/// unsupported boundary.
///
/// Verified in reference: `game/mission/game.cpp` belongs to Game.exe and
/// the marker is `// STUB: GAME 0x5baa40` (code, not a data offset and not
/// another binary). RVA `0x1baa40` = `0x5baa40 - 0x400000`; VA
/// `0x400000 + 0x1baa40` = `0x5baa40`. The RVA appears in neither
/// `docs/rewrite-overlay.json` nor `docs/progress.json`.
pub const REMOVE_DEATH_HUMAN_FADE_ENTRY: u32 = 0x5baa40;

/// Original `C_game::RemoveDeathHumanFade` stub: always reports the
/// boundary instead of inventing fade ownership.
pub fn remove_death_human_fade(_index: i32, _flag: bool) -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: REMOVE_DEATH_HUMAN_FADE_ENTRY,
    })
}

/// STUB: GAME RVA 0x1b7f70, VA 0x5b7f70 (`game.cpp:3804-3808`,
/// `game.h:809`, `C_game::AddInvisibleActor`): unwritten upstream
/// (`StubLog` only). The invisible-actor list and the actor owner stay
/// with their owners (DEFERRED). Explicit unsupported boundary.
///
/// Verified in reference: `game/mission/game.cpp` belongs to Game.exe and
/// the marker is `// STUB: GAME 0x5b7f70` (code, not a data offset and not
/// another binary). RVA `0x1b7f70` = `0x5b7f70 - 0x400000`; VA
/// `0x400000 + 0x1b7f70` = `0x5b7f70`. The RVA appears in neither
/// `docs/rewrite-overlay.json` nor `docs/progress.json`.
pub const ADD_INVISIBLE_ACTOR_ENTRY: u32 = 0x5b7f70;

/// Original `C_game::AddInvisibleActor` stub: always reports the boundary
/// instead of inventing the invisible-actor owner.
pub fn add_invisible_actor(_actor: ActorId) -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: ADD_INVISIBLE_ACTOR_ENTRY,
    })
}

/// STUB: GAME RVA 0x1b8150, VA 0x5b8150 (`game.cpp:3810-3814`,
/// `game.h:673`, `C_game::FindActorsVect`): unwritten upstream (`StubLog`
/// only). The actor-vector scan stays with the mission owner (DEFERRED).
/// Explicit unsupported boundary.
///
/// Verified in reference: `game/mission/game.cpp` belongs to Game.exe and
/// the marker is `// STUB: GAME 0x5b8150` (code, not a data offset and not
/// another binary). RVA `0x1b8150` = `0x5b8150 - 0x400000`; VA
/// `0x400000 + 0x1b8150` = `0x5b8150`. The RVA appears in neither
/// `docs/rewrite-overlay.json` nor `docs/progress.json`.
pub const FIND_ACTORS_VECT_ENTRY: u32 = 0x5b8150;

/// Original `C_game::FindActorsVect` stub: always reports the boundary
/// instead of inventing the actor-vector scan.
pub fn find_actors_vect() -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: FIND_ACTORS_VECT_ENTRY,
    })
}

/// STUB: GAME RVA 0x1b81e0, VA 0x5b81e0 (`game.cpp:3816-3820`,
/// `game.h:812`, `C_game::AddCutsceneParticle`): unwritten upstream
/// (`StubLog` only). The cutscene-particle list stays with the cutscene
/// owner (DEFERRED). Explicit unsupported boundary.
///
/// Verified in reference: `game/mission/game.cpp` belongs to Game.exe and
/// the marker is `// STUB: GAME 0x5b81e0` (code, not a data offset and not
/// another binary). RVA `0x1b81e0` = `0x5b81e0 - 0x400000`; VA
/// `0x400000 + 0x1b81e0` = `0x5b81e0`. The RVA appears in neither
/// `docs/rewrite-overlay.json` nor `docs/progress.json`.
pub const ADD_CUTSCENE_PARTICLE_ENTRY: u32 = 0x5b81e0;

/// Original `C_game::AddCutsceneParticle` stub: always reports the
/// boundary instead of inventing cutscene-particle ownership.
pub fn add_cutscene_particle(_particle: i32) -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: ADD_CUTSCENE_PARTICLE_ENTRY,
    })
}

/// STUB: GAME RVA 0x1b8390, VA 0x5b8390 (`game.cpp:3822-3826`,
/// `game.h:813`, `C_game::DestroyCutsceneParticles`): unwritten upstream
/// (`StubLog` only). The cutscene-particle list stays with the cutscene
/// owner (DEFERRED). Explicit unsupported boundary.
///
/// Verified in reference: `game/mission/game.cpp` belongs to Game.exe and
/// the marker is `// STUB: GAME 0x5b8390` (code, not a data offset and not
/// another binary). RVA `0x1b8390` = `0x5b8390 - 0x400000`; VA
/// `0x400000 + 0x1b8390` = `0x5b8390`. The RVA appears in neither
/// `docs/rewrite-overlay.json` nor `docs/progress.json`.
pub const DESTROY_CUTSCENE_PARTICLES_ENTRY: u32 = 0x5b8390;

/// Original `C_game::DestroyCutsceneParticles` stub: always reports the
/// boundary instead of inventing cutscene-particle ownership.
pub fn destroy_cutscene_particles() -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: DESTROY_CUTSCENE_PARTICLES_ENTRY,
    })
}

/// STUB: GAME RVA 0x1b8e70, VA 0x5b8e70 (`game.cpp:3836-3840`,
/// `game.h:816`, `C_game::AddCreatedFrameToVector`): unwritten upstream
/// (`StubLog` only). The created-frames vector stays with the scene owner
/// (DEFERRED). Explicit unsupported boundary.
///
/// Verified in reference: `game/mission/game.cpp` belongs to Game.exe and
/// the marker is `// STUB: GAME 0x5b8e70` (code, not a data offset and not
/// another binary). RVA `0x1b8e70` = `0x5b8e70 - 0x400000`; VA
/// `0x400000 + 0x1b8e70` = `0x5b8e70`. The RVA appears in neither
/// `docs/rewrite-overlay.json` nor `docs/progress.json`.
pub const ADD_CREATED_FRAME_TO_VECTOR_ENTRY: u32 = 0x5b8e70;

/// Original `C_game::AddCreatedFrameToVector` stub: always reports the
/// boundary instead of inventing created-frame ownership.
pub fn add_created_frame_to_vector(_frame: FrameId) -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: ADD_CREATED_FRAME_TO_VECTOR_ENTRY,
    })
}

/// STUB: GAME RVA 0x1b9040, VA 0x5b9040 (`game.cpp:3842-3847`,
/// `game.h:817`, `C_game::DelCreatedFrameToVector`): unwritten upstream
/// (`StubLog` + `false`). The created-frames vector stays with the scene
/// owner (DEFERRED); the host mirrors the native `false` without inventing
/// the removal, like `is_valid_taxi_passenger` (GAME 0x5b6d10).
///
/// Verified in reference: `game/mission/game.cpp` belongs to Game.exe and
/// the marker is `// STUB: GAME 0x5b9040` (code, not a data offset and not
/// another binary). RVA `0x1b9040` = `0x5b9040 - 0x400000`; VA
/// `0x400000 + 0x1b9040` = `0x5b9040`. The RVA appears in neither
/// `docs/rewrite-overlay.json` nor `docs/progress.json`.
pub fn del_created_frame_to_vector(_frame: FrameId) -> bool {
    false
}

/// STUB: GAME RVA 0x1ba010, VA 0x5ba010 (`game.cpp:3976-3980`,
/// `game.h:820`, `C_game::SetCameraRotRepair`): unwritten upstream
/// (`StubLog` only). The camera-rotation repair flag stays with the camera
/// owner (DEFERRED). Explicit unsupported boundary.
///
/// Verified in reference: `game/mission/game.cpp` belongs to Game.exe and
/// the marker is `// STUB: GAME 0x5ba010` (code, not a data offset and not
/// another binary). RVA `0x1ba010` = `0x5ba010 - 0x400000`; VA
/// `0x400000 + 0x1ba010` = `0x5ba010`. The RVA appears in neither
/// `docs/rewrite-overlay.json` nor `docs/progress.json`.
pub const SET_CAMERA_ROT_REPAIR_ENTRY: u32 = 0x5ba010;

/// Original `C_game::SetCameraRotRepair` stub: always reports the boundary
/// instead of inventing camera ownership.
pub fn set_camera_rot_repair() -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: SET_CAMERA_ROT_REPAIR_ENTRY,
    })
}

/// STUB: GAME RVA 0x1ba080, VA 0x5ba080 (`game.cpp:3982-3986`,
/// `game.h:821`, `C_game::ClearCartridges`): unwritten upstream (`StubLog`
/// only). The cartridge frames stay with the actor owner (DEFERRED).
/// Explicit unsupported boundary.
///
/// Verified in reference: `game/mission/game.cpp` belongs to Game.exe and
/// the marker is `// STUB: GAME 0x5ba080` (code, not a data offset and not
/// another binary). RVA `0x1ba080` = `0x5ba080 - 0x400000`; VA
/// `0x400000 + 0x1ba080` = `0x5ba080`. The RVA appears in neither
/// `docs/rewrite-overlay.json` nor `docs/progress.json`.
pub const CLEAR_CARTRIDGES_ENTRY: u32 = 0x5ba080;

/// Original `C_game::ClearCartridges` stub: always reports the boundary
/// instead of inventing cartridge ownership.
pub fn clear_cartridges() -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: CLEAR_CARTRIDGES_ENTRY,
    })
}

/// STUB: GAME RVA 0x1ba110, VA 0x5ba110 (`game.cpp:3988-3992`,
/// `game.h:674`, `C_game::TickCarSoundManager`): unwritten upstream
/// (`StubLog` only). The car-sound tick stays with the sound owner
/// (DEFERRED). Explicit unsupported boundary.
///
/// Verified in reference: `game/mission/game.cpp` belongs to Game.exe and
/// the marker is `// STUB: GAME 0x5ba110` (code, not a data offset and not
/// another binary). RVA `0x1ba110` = `0x5ba110 - 0x400000`; VA
/// `0x400000 + 0x1ba110` = `0x5ba110`. The RVA appears in neither
/// `docs/rewrite-overlay.json` nor `docs/progress.json`.
pub const TICK_CAR_SOUND_MANAGER_ENTRY: u32 = 0x5ba110;

/// Original `C_game::TickCarSoundManager` stub: always reports the
/// boundary instead of inventing car-sound ownership.
pub fn tick_car_sound_manager() -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: TICK_CAR_SOUND_MANAGER_ENTRY,
    })
}

/// STUB: GAME RVA 0x1b5450, VA 0x5b5450 (`game.cpp:4127-4131`,
/// `C_game::LightnessAdd`): unwritten upstream (`StubLog` only). The
/// lightness list and the actor/frame owners stay with their owners
/// (DEFERRED). Explicit unsupported boundary.
///
/// Verified in reference: `game/mission/game.cpp` belongs to Game.exe and
/// the marker is `// STUB: GAME 0x5b5450` (code, not a data offset and not
/// another binary). RVA `0x1b5450` = `0x5b5450 - 0x400000`; VA
/// `0x400000 + 0x1b5450` = `0x5b5450`. The RVA is not taken as a ported
/// entry by `docs/rewrite-overlay.json` or by any Rust symbol in this tree
/// (the frozen `docs/progress.json` inventory row carries no port).
pub const LIGHTNESS_ADD_ENTRY: u32 = 0x5b5450;

/// Original `C_game::LightnessAdd` stub: always reports the boundary
/// instead of inventing lightness ownership.
pub fn lightness_add(
    _actor: ActorId,
    _frame: FrameId,
    _unknown: u16,
    _pos: [f32; 3],
) -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: LIGHTNESS_ADD_ENTRY,
    })
}

/// STUB: GAME RVA 0x1b56f0, VA 0x5b56f0 (`game.cpp:4133-4137`,
/// `C_game::LightnessDel`): unwritten upstream (`StubLog` only). The
/// lightness list stays with its owner (DEFERRED). Explicit unsupported
/// boundary.
///
/// Verified in reference: `game/mission/game.cpp` belongs to Game.exe and
/// the marker is `// STUB: GAME 0x5b56f0` (code, not a data offset and not
/// another binary). RVA `0x1b56f0` = `0x5b56f0 - 0x400000`; VA
/// `0x400000 + 0x1b56f0` = `0x5b56f0`. The RVA is not taken as a ported
/// entry by `docs/rewrite-overlay.json` or by any Rust symbol in this tree
/// (the frozen `docs/progress.json` inventory row carries no port).
pub const LIGHTNESS_DEL_ENTRY: u32 = 0x5b56f0;

/// Original `C_game::LightnessDel` stub: always reports the boundary
/// instead of inventing lightness ownership.
pub fn lightness_del(_frame: FrameId) -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: LIGHTNESS_DEL_ENTRY,
    })
}

/// STUB: GAME RVA 0x1b7be0, VA 0x5b7be0 (`game.cpp:3771-3776`,
/// `C_game::DestroyControlSignal`): unwritten upstream (`StubLog` only).
/// The collision object, the signal frames and the human owner stay with
/// their owners (DEFERRED). Explicit unsupported boundary.
///
/// Verified in reference: `game/mission/game.cpp` belongs to Game.exe and
/// the marker is `// STUB: GAME 0x5b7be0` (code, not a data offset and not
/// another binary). RVA `0x1b7be0` = `0x5b7be0 - 0x400000`; VA
/// `0x400000 + 0x1b7be0` = `0x5b7be0`. The RVA is not taken as a ported
/// entry by `docs/rewrite-overlay.json` or by any Rust symbol in this tree
/// (the frozen `docs/progress.json` inventory row carries no port).
pub const DESTROY_CONTROL_SIGNAL_ENTRY: u32 = 0x5b7be0;

/// Original `C_game::DestroyControlSignal` stub: always reports the
/// boundary instead of inventing collision/signal ownership.
pub fn destroy_control_signal(
    _coll: u32,
    _pos: [f32; 3],
    _from: [f32; 3],
    _dir: [f32; 3],
    _human: ActorId,
) -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: DESTROY_CONTROL_SIGNAL_ENTRY,
    })
}

/// STUB: GAME RVA 0x1b9690, VA 0x5b9690 (`game.cpp:4203-4206`,
/// `C_game::AddDeadHumanForTick`): unwritten upstream (`StubLog` only).
/// The dead-human fade list and the human owner stay with their owners
/// (DEFERRED). Explicit unsupported boundary.
///
/// Verified in reference: `game/mission/game.cpp` belongs to Game.exe and
/// the marker is `// STUB: GAME 0x5b9690` (code, not a data offset and not
/// another binary). RVA `0x1b9690` = `0x5b9690 - 0x400000`; VA
/// `0x400000 + 0x1b9690` = `0x5b9690`. The RVA is not taken as a ported
/// entry by `docs/rewrite-overlay.json` or by any Rust symbol in this tree
/// (the frozen `docs/progress.json` inventory row carries no port).
pub const ADD_DEAD_HUMAN_FOR_TICK_ENTRY: u32 = 0x5b9690;

/// Original `C_game::AddDeadHumanForTick` stub: always reports the boundary
/// instead of inventing fade-list ownership.
pub fn add_dead_human_for_tick(_human: ActorId) -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: ADD_DEAD_HUMAN_FOR_TICK_ENTRY,
    })
}

/// STUB: GAME RVA 0x1b98c0, VA 0x5b98c0 (`game.cpp:3934-3938`,
/// `C_game::TickDeadHumanForTick`): unwritten upstream (`StubLog` only).
/// The dead-human fade tick stays with its owner (DEFERRED). Explicit
/// unsupported boundary.
///
/// Verified in reference: `game/mission/game.cpp` belongs to Game.exe and
/// the marker is `// STUB: GAME 0x5b98c0` (code, not a data offset and not
/// another binary). RVA `0x1b98c0` = `0x5b98c0 - 0x400000`; VA
/// `0x400000 + 0x1b98c0` = `0x5b98c0`. The RVA is not taken as a ported
/// entry by `docs/rewrite-overlay.json` or by any Rust symbol in this tree
/// (the frozen `docs/progress.json` inventory row carries no port).
pub const TICK_DEAD_HUMAN_FOR_TICK_ENTRY: u32 = 0x5b98c0;

/// Original `C_game::TickDeadHumanForTick` stub: always reports the
/// boundary instead of inventing fade-tick ownership.
pub fn tick_dead_human_for_tick(_time: u32) -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: TICK_DEAD_HUMAN_FOR_TICK_ENTRY,
    })
}

/// STUB: GAME RVA 0x1ba390, VA 0x5ba390 (`game.cpp:4004-4008`,
/// `C_game::ItemBlurTick`): unwritten upstream (`StubLog` only). The blur
/// switch it reads (`item_blur`, cleared by the ported
/// `ClearDeathHumanFade`, GAME 0x5ba310) and the effect owner stay with
/// their owners (DEFERRED). Explicit unsupported boundary.
///
/// Verified in reference: `game/mission/game.cpp` belongs to Game.exe and
/// the marker is `// STUB: GAME 0x5ba390` (code, not a data offset and not
/// another binary). RVA `0x1ba390` = `0x5ba390 - 0x400000`; VA
/// `0x400000 + 0x1ba390` = `0x5ba390`. The RVA is not taken as a ported
/// entry by `docs/rewrite-overlay.json` or by any Rust symbol in this tree
/// (the frozen `docs/progress.json` inventory row carries no port).
pub const ITEM_BLUR_TICK_ENTRY: u32 = 0x5ba390;

/// Original `C_game::ItemBlurTick` stub: always reports the boundary
/// instead of inventing effect ownership.
pub fn item_blur_tick(_time: u32) -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: ITEM_BLUR_TICK_ENTRY,
    })
}

/// STUB: GAME RVA 0x1bab20, VA 0x5bab20 (`game.cpp:4016-4020`,
/// `C_game::RecPrepareHumans`): unwritten upstream (`StubLog` only). The
/// human/recording preparation stays with the actor owner (DEFERRED).
/// Explicit unsupported boundary.
///
/// Verified in reference: `game/mission/game.cpp` belongs to Game.exe and
/// the marker is `// STUB: GAME 0x5bab20` (code, not a data offset and not
/// another binary). RVA `0x1bab20` = `0x5bab20 - 0x400000`; VA
/// `0x400000 + 0x1bab20` = `0x5bab20`. The RVA is not taken as a ported
/// entry by `docs/rewrite-overlay.json` or by any Rust symbol in this tree
/// (the frozen `docs/progress.json` inventory row carries no port).
pub const REC_PREPARE_HUMANS_ENTRY: u32 = 0x5bab20;

/// Original `C_game::RecPrepareHumans` stub: always reports the boundary
/// instead of inventing human preparation.
pub fn rec_prepare_humans() -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: REC_PREPARE_HUMANS_ENTRY,
    })
}

/// STUB: GAME RVA 0x1bb570, VA 0x5bb570 (`game.cpp:4075-4080`, the static
/// `AddInterCityBox` helper of `C_game::InterCity_GameInit`): unwritten
/// upstream (`StubLog` only). The collision arrays, matrices, frames and
/// the scene dummy frame stay with the collision and scene owners
/// (DEFERRED), so no parameter is staged here. Explicit unsupported
/// boundary.
///
/// Verified in reference: `game/mission/game.cpp` belongs to Game.exe and
/// the marker is `// STUB: GAME 0x5bb570` (code, not a data offset and not
/// another binary). RVA `0x1bb570` = `0x5bb570 - 0x400000`; VA
/// `0x400000 + 0x1bb570` = `0x5bb570`. The RVA is not taken as a ported
/// entry by `docs/rewrite-overlay.json` or by any Rust symbol in this tree
/// (the frozen `docs/progress.json` inventory row carries no port).
pub const ADD_INTER_CITY_BOX_ENTRY: u32 = 0x5bb570;

/// Original `AddInterCityBox` stub: always reports the boundary instead of
/// inventing collision/scene ownership.
pub fn add_inter_city_box() -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: ADD_INTER_CITY_BOX_ENTRY,
    })
}

/// STUB: GAME RVA 0x0aa7a0, VA 0x4aa7a0 (`game/vehicles/C_trf_c.cpp:23-27`,
/// `C_traffic_car::C_traffic_car`): unwritten upstream (`StubLog` only).
/// Traffic-car construction stays with the actor owner (DEFERRED).
/// Explicit unsupported boundary.
///
/// Verified in reference: `game/vehicles/C_trf_c.cpp` belongs to Game.exe
/// and the marker is `// STUB: GAME 0x4aa7a0` (code, not a data offset and
/// not another binary). RVA `0x0aa7a0` = `0x4aa7a0 - 0x400000`; VA
/// `0x400000 + 0x0aa7a0` = `0x4aa7a0`. The RVA is not taken as a ported
/// entry by `docs/rewrite-overlay.json` or by any Rust symbol in this tree
/// (the frozen `docs/progress.json` inventory row carries no port).
pub const TRAFFIC_CAR_CTOR_ENTRY: u32 = 0x4aa7a0;

/// Original `C_traffic_car::C_traffic_car` stub: always reports the
/// boundary instead of inventing actor ownership.
pub fn traffic_car_ctor() -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: TRAFFIC_CAR_CTOR_ENTRY,
    })
}

/// STUB: GAME RVA 0x0aa830, VA 0x4aa830 (`game/vehicles/C_trf_c.cpp:53-57`,
/// `C_traffic_car::~C_traffic_car`): unwritten upstream (`StubLog` only).
/// Traffic-car destruction stays with the actor owner (DEFERRED). Explicit
/// unsupported boundary.
///
/// Verified in reference: `game/vehicles/C_trf_c.cpp` belongs to Game.exe
/// and the marker is `// STUB: GAME 0x4aa830` (code, not a data offset and
/// not another binary). RVA `0x0aa830` = `0x4aa830 - 0x400000`; VA
/// `0x400000 + 0x0aa830` = `0x4aa830`. The RVA is not taken as a ported
/// entry by `docs/rewrite-overlay.json` or by any Rust symbol in this tree
/// (the frozen `docs/progress.json` inventory row carries no port).
pub const TRAFFIC_CAR_DTOR_ENTRY: u32 = 0x4aa830;

/// Original `C_traffic_car::~C_traffic_car` stub: always reports the
/// boundary instead of inventing actor ownership.
pub fn traffic_car_dtor() -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: TRAFFIC_CAR_DTOR_ENTRY,
    })
}

/// STUB: GAME RVA 0x0aa860, VA 0x4aa860 (`game/vehicles/C_trf_c.cpp:59-64`,
/// `C_traffic_car::Init`): unwritten upstream (`StubLog` only, then the
/// `C_actor::Init` base call). The actor init stays with the actor owner
/// (DEFERRED), so the base-call result is unobservable here. Explicit
/// unsupported boundary.
///
/// Verified in reference: `game/vehicles/C_trf_c.cpp` belongs to Game.exe
/// and the marker is `// STUB: GAME 0x4aa860` (code, not a data offset and
/// not another binary). RVA `0x0aa860` = `0x4aa860 - 0x400000`; VA
/// `0x400000 + 0x0aa860` = `0x4aa860`. The RVA is not taken as a ported
/// entry by `docs/rewrite-overlay.json` or by any Rust symbol in this tree
/// (the frozen `docs/progress.json` inventory row carries no port).
pub const TRAFFIC_CAR_INIT_ENTRY: u32 = 0x4aa860;

/// Original `C_traffic_car::Init` stub: always reports the boundary instead
/// of inventing actor ownership.
pub fn traffic_car_init(_frame: FrameId) -> Result<bool, UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: TRAFFIC_CAR_INIT_ENTRY,
    })
}

/// STUB: GAME RVA 0x0aa8b0, VA 0x4aa8b0 (`game/vehicles/C_trf_c.cpp:66-71`,
/// `C_traffic_car::ChangeState`): unwritten upstream (`StubLog` only, then
/// the `C_actor::ChangeState` base call). The actor state stays with the
/// actor owner (DEFERRED). Explicit unsupported boundary.
///
/// Verified in reference: `game/vehicles/C_trf_c.cpp` belongs to Game.exe
/// and the marker is `// STUB: GAME 0x4aa8b0` (code, not a data offset and
/// not another binary). RVA `0x0aa8b0` = `0x4aa8b0 - 0x400000`; VA
/// `0x400000 + 0x0aa8b0` = `0x4aa8b0`. The RVA is not taken as a ported
/// entry by `docs/rewrite-overlay.json` or by any Rust symbol in this tree
/// (the frozen `docs/progress.json` inventory row carries no port).
pub const TRAFFIC_CAR_CHANGE_STATE_ENTRY: u32 = 0x4aa8b0;

/// Original `C_traffic_car::ChangeState` stub: always reports the boundary
/// instead of inventing actor ownership.
pub fn traffic_car_change_state() -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: TRAFFIC_CAR_CHANGE_STATE_ENTRY,
    })
}

/// STUB: GAME RVA 0x0b54a0, VA 0x4b54a0 (`game/vehicles/C_trf_c.cpp:131-136`,
/// `C_traffic_car::GameInit`): unwritten upstream (`StubLog` only, then the
/// `C_actor::GameInit` base call). The actor game-init stays with the actor
/// owner (DEFERRED). Explicit unsupported boundary.
///
/// Verified in reference: `game/vehicles/C_trf_c.cpp` belongs to Game.exe
/// and the marker is `// STUB: GAME 0x4b54a0` (code, not a data offset and
/// not another binary). RVA `0x0b54a0` = `0x4b54a0 - 0x400000`; VA
/// `0x400000 + 0x0b54a0` = `0x4b54a0`. The RVA is not taken as a ported
/// entry by `docs/rewrite-overlay.json` or by any Rust symbol in this tree
/// (the frozen `docs/progress.json` inventory row carries no port).
pub const TRAFFIC_CAR_GAME_INIT_ENTRY: u32 = 0x4b54a0;

/// Original `C_traffic_car::GameInit` stub: always reports the boundary
/// instead of inventing actor ownership.
pub fn traffic_car_game_init() -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: TRAFFIC_CAR_GAME_INIT_ENTRY,
    })
}

/// STUB: GAME RVA 0x0b8010, VA 0x4b8010 (`game/vehicles/C_trf_c.cpp:138-143`,
/// `C_traffic_car::GameDone`): unwritten upstream (`StubLog` only, then the
/// `C_actor::GameDone` base call). The actor game-done stays with the actor
/// owner (DEFERRED). Explicit unsupported boundary.
///
/// Verified in reference: `game/vehicles/C_trf_c.cpp` belongs to Game.exe
/// and the marker is `// STUB: GAME 0x4b8010` (code, not a data offset and
/// not another binary). RVA `0x0b8010` = `0x4b8010 - 0x400000`; VA
/// `0x400000 + 0x0b8010` = `0x4b8010`. The RVA is not taken as a ported
/// entry by `docs/rewrite-overlay.json` or by any Rust symbol in this tree
/// (the frozen `docs/progress.json` inventory row carries no port).
pub const TRAFFIC_CAR_GAME_DONE_ENTRY: u32 = 0x4b8010;

/// Original `C_traffic_car::GameDone` stub: always reports the boundary
/// instead of inventing actor ownership.
pub fn traffic_car_game_done() -> Result<(), UnwrittenGameEntry> {
    Err(UnwrittenGameEntry::Unsupported {
        entry: TRAFFIC_CAR_GAME_DONE_ENTRY,
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
    fn rva_1b6b60_find_taxi_points_pushes_every_frame() {
        // C_game::S_hlp::cbFindTaxiPoints GAME RVA 0x1b6b60 (VA 0x5b6b60).
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
    fn telephone_model_indices_scan_back_and_keep_lowest_match() {
        // Pure model-set fragment of C_game::LoadTelephones (GAME 0x5b6e00).
        assert_eq!(
            telephone_model_indices(&["alarm01.i3d", "PHbox01.i3d"]),
            (Some(0), Some(1))
        );
        // Scanning from the back with plain assignment keeps the lowest
        // matching index; matching is case-insensitive past the last '\'.
        assert_eq!(
            telephone_model_indices(&[
                "models\\PHBOX01.I3D",
                "other.i3d",
                "alarm01.i3d",
                "ALARM01.I3D",
            ]),
            (Some(2), Some(0))
        );
        assert_eq!(
            telephone_model_indices(&["other.i3d"] as &[&str]),
            (None, None)
        );
        assert_eq!(telephone_model_indices(&[] as &[&str]), (None, None));
    }

    #[test]
    fn collect_block_telephones_pushes_null_frame_records_back_to_front() {
        // Pure block fragment of C_game::LoadTelephones (GAME 0x5b6e00).
        let out = collect_block_telephones(Some(3), Some(7), &[vec![1, 3], vec![7, 3, 9]]);
        // Back block first, back model first, alarm arm before box arm.
        assert_eq!(
            out,
            vec![
                TelephoneEntry {
                    frame: TELEPHONE_NULL_FRAME,
                    phone_box: false,
                },
                TelephoneEntry {
                    frame: TELEPHONE_NULL_FRAME,
                    phone_box: true,
                },
                TelephoneEntry {
                    frame: TELEPHONE_NULL_FRAME,
                    phone_box: false,
                },
            ]
        );
        // No indices, no records; unmatched models push nothing.
        assert!(collect_block_telephones(None, None, &[vec![3, 7]]).is_empty());
        assert!(collect_block_telephones(Some(3), Some(7), &[]).is_empty());
        assert_eq!(TELEPHONE_NULL_FRAME, 0);
    }

    #[test]
    fn get_cheats_bit6_reads_bit_six_only() {
        // Inline C_game::GetCheatsBit6 (game.h:834-840, no outline RVA).
        let mut state = GameInlineState::default();
        assert!(!state.get_cheats_bit6());
        state.set_cheats(1 << 6);
        assert!(state.get_cheats_bit6());
        state.set_cheats(u32::MAX & !(1 << 6));
        assert!(!state.get_cheats_bit6());
        state.set_cheats(u32::MAX);
        assert!(state.get_cheats_bit6());
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

    #[test]
    fn rva_7b510_get_camera_returns_staged_handle() {
        // C_game::GetCamera GAME RVA 0x7b510 (VA 0x47b510).
        let mut state = GameInlineState::default();
        assert_eq!(state.get_camera(), None);
        state.stage_camera(Some(0xabcd));
        assert_eq!(state.get_camera(), Some(0xabcd));
        state.stage_camera(None);
        assert_eq!(state.get_camera(), None);
    }

    #[test]
    fn rva_7b530_get_using_object_returns_staged_handle() {
        // C_game::GetUsingObject GAME RVA 0x7b530 (VA 0x47b530).
        let mut state = GameInlineState::default();
        assert_eq!(state.get_using_object(), None);
        state.stage_using_object(Some(41));
        assert_eq!(state.get_using_object(), Some(41));
    }

    #[test]
    fn rva_7b550_get_sledovacka_returns_staged_handle() {
        // C_game::GetSledovacka GAME RVA 0x7b550 (VA 0x47b550).
        let mut state = GameInlineState::default();
        assert_eq!(state.get_sledovacka(), None);
        state.stage_sledovacka(Some(7));
        assert_eq!(state.get_sledovacka(), Some(7));
        state.stage_sledovacka(None);
        assert_eq!(state.get_sledovacka(), None);
    }

    #[test]
    fn rva_7b580_get_sound_arena_returns_staged_handle() {
        // C_game::GetSoundArena GAME RVA 0x7b580 (VA 0x47b580).
        let mut state = GameInlineState::default();
        assert_eq!(state.get_sound_arena(), None);
        state.stage_sound_arena(Some(u32::MAX));
        assert_eq!(state.get_sound_arena(), Some(u32::MAX));
    }

    #[test]
    fn rva_7b620_get_shvestky_returns_staged_handle() {
        // C_game::GetShvestky GAME RVA 0x7b620 (VA 0x47b620).
        let mut state = GameInlineState::default();
        assert_eq!(state.get_shvestky(), None);
        state.stage_shvestky(Some(99));
        assert_eq!(state.get_shvestky(), Some(99));
        state.stage_shvestky(None);
        assert_eq!(state.get_shvestky(), None);
    }

    #[test]
    fn rva_7b680_set_actor_lightness_stores_switch() {
        // C_game::SetActorLightness GAME RVA 0x7b680 (VA 0x47b680).
        let mut state = GameInlineState::default();
        assert!(!state.actor_lightness());
        state.set_actor_lightness(true);
        assert!(state.actor_lightness());
        state.set_actor_lightness(false);
        assert!(!state.actor_lightness());
    }

    #[test]
    fn rva_7b690_set_anim_preload_stores_switch() {
        // C_game::SetAnimPreload GAME RVA 0x7b690 (VA 0x47b690).
        let mut state = GameInlineState::default();
        assert!(!state.anim_preload());
        state.set_anim_preload(true);
        assert!(state.anim_preload());
        state.set_anim_preload(false);
        assert!(!state.anim_preload());
    }

    #[test]
    fn rva_7b6a0_set_weather_buffer_stores_value() {
        // C_game::SetWeatherBuffer GAME RVA 0x7b6a0 (VA 0x47b6a0).
        let mut state = GameInlineState::default();
        assert_eq!(state.weather_buffer(), 0);
        state.set_weather_buffer(-3);
        assert_eq!(state.weather_buffer(), -3);
        state.set_weather_buffer(i32::MAX);
        assert_eq!(state.weather_buffer(), i32::MAX);
    }

    #[test]
    fn rva_7b6b0_get_score_reads_stored_score() {
        // C_game::GetScore GAME RVA 0x7b6b0 (VA 0x47b6b0).
        let state = ScoreState { score: -12, score_on: false };
        assert_eq!(state.get_score(), -12);
        let state = ScoreState { score: 0, score_on: true };
        assert_eq!(state.get_score(), 0);
        let state = ScoreState { score: i32::MAX, score_on: true };
        assert_eq!(state.get_score(), i32::MAX);
    }

    #[test]
    fn rva_1add70_free_semaphores_lights_all_on_then_clears() {
        // C_game::FreeSemaphores GAME RVA 0x1add70 (VA 0x5add70).
        let mut semaphores = vec![
            SemaphoreEntry { red: Some(1), orange: None, green: Some(3), use_phase_x: true },
            SemaphoreEntry { red: None, orange: Some(4), green: None, use_phase_x: false },
        ];
        let mut backend = SemaphoreLog { states: vec![] };
        free_semaphores(&mut semaphores, &mut backend);
        assert!(semaphores.is_empty());
        // Back entry first, every present lamp switched on.
        assert_eq!(backend.states, vec![(4, true), (1, true), (3, true)]);
        // Empty input drives no lamp and stays empty.
        let mut backend = SemaphoreLog { states: vec![] };
        free_semaphores(&mut semaphores, &mut backend);
        assert!(backend.states.is_empty());
    }

    #[test]
    fn rva_7b590_set_night_mission_stores_switch() {
        // C_game::SetNightMission GAME RVA 0x7b590 (VA 0x47b590).
        let mut state = GameInlineState::default();
        assert!(!state.night_mission());
        state.set_night_mission(true);
        assert!(state.night_mission());
        state.set_night_mission(false);
        assert!(!state.night_mission());
    }

    #[test]
    fn rva_7b670_set_race6_result_stores_handle() {
        // C_game::SetRace6Result GAME RVA 0x7b670 (VA 0x47b670).
        let mut state = GameInlineState::default();
        assert_eq!(state.race6_result(), None);
        state.set_race6_result(Some(7));
        assert_eq!(state.race6_result(), Some(7));
        state.set_race6_result(None);
        assert_eq!(state.race6_result(), None);
    }

    #[test]
    fn rva_147a60_game_drop_stacks_empties_all_three() {
        // ~C_game GAME RVA 0x147a60 (VA 0x547a60).
        let mut inventory = vec![InventorySnapshot { marker: 1 }, InventorySnapshot { marker: 2 }];
        let mut character = vec![3u32, 4u32];
        let mut floats = vec![0.5f32];
        game_drop_stacks(&mut inventory, &mut character, &mut floats);
        assert!(inventory.is_empty());
        assert!(character.is_empty());
        assert!(floats.is_empty());
        // Empty stacks stay empty.
        game_drop_stacks(&mut inventory, &mut character, &mut floats);
        assert!(inventory.is_empty() && character.is_empty() && floats.is_empty());
    }

    struct RemovedLog {
        none: Vec<ActorId>,
    }

    impl RemovedActorBackend for RemovedLog {
        fn set_act_state_none(&mut self, actor: ActorId) {
            self.none.push(actor);
        }
    }

    #[test]
    fn rva_1a79a0_remove_temporary_actor_queues_missing() {
        // C_game::RemoveTemporaryActor GAME RVA 0x1a79a0 (VA 0x5a79a0).
        let mut removed = vec![9u32];
        let mut backend = RemovedLog { none: vec![] };
        remove_temporary_actor(&mut removed, 4, &mut backend);
        assert_eq!(removed, vec![9, 4]);
        assert_eq!(backend.none, vec![4]);
    }

    #[test]
    fn rva_1a79a0_remove_temporary_actor_skips_listed() {
        // C_game::RemoveTemporaryActor GAME RVA 0x1a79a0 (VA 0x5a79a0).
        let mut removed = vec![4u32];
        let mut backend = RemovedLog { none: vec![] };
        remove_temporary_actor(&mut removed, 4, &mut backend);
        assert_eq!(removed, vec![4]);
        assert!(backend.none.is_empty());
    }

    struct TemporaryLog {
        order: Vec<(char, ActorId)>,
    }

    impl TemporaryActorBackend for TemporaryLog {
        fn invalidate_actor(&mut self, actor: ActorId) {
            self.order.push(('i', actor));
        }
        fn actor_game_done(&mut self, actor: ActorId) {
            self.order.push(('g', actor));
        }
        fn release_actor(&mut self, actor: ActorId) {
            self.order.push(('r', actor));
        }
    }

    #[test]
    fn rva_1a7f60_clear_temporary_actors_releases_unkept_back_first() {
        // C_game::ClearTemporaryActors GAME RVA 0x1a7f60 (VA 0x5a7f60).
        let mut temporary = vec![1u32, 2, 3];
        let mut backend = TemporaryLog { order: vec![] };
        clear_temporary_actors(&mut temporary, [Some(2), None], &mut backend);
        assert!(temporary.is_empty());
        // Back first, kept actor skipped, invalidate/GameDone/release per actor.
        assert_eq!(
            backend.order,
            vec![('i', 3), ('g', 3), ('r', 3), ('i', 1), ('g', 1), ('r', 1)]
        );
    }

    #[test]
    fn rva_1a7f60_clear_temporary_actors_empty_is_noop() {
        // C_game::ClearTemporaryActors GAME RVA 0x1a7f60 (VA 0x5a7f60).
        let mut temporary: Vec<ActorId> = vec![];
        let mut backend = TemporaryLog { order: vec![] };
        clear_temporary_actors(&mut temporary, [Some(1), Some(2)], &mut backend);
        assert!(temporary.is_empty());
        assert!(backend.order.is_empty());
    }

    fn shadow_template(name: Option<&str>) -> ShadowTemplateEntry {
        ShadowTemplateEntry { name: name.map(str::to_owned) }
    }

    #[test]
    fn rva_1ad210_setup_shadow_select_prefers_last_name_match() {
        // C_game::SetupShadow GAME RVA 0x1ad210 (VA 0x5ad210).
        let templates = vec![
            shadow_template(Some("car")),
            shadow_template(Some("CAR")),
            shadow_template(None),
            shadow_template(Some("tree")),
        ];
        // Last match wins, ASCII-case-insensitive.
        assert_eq!(
            setup_shadow_select(&templates, Some("car"), 0),
            ShadowDuplicateSource::Template(1)
        );
        assert_eq!(
            setup_shadow_select(&templates, Some("TREE"), 0),
            ShadowDuplicateSource::Template(3)
        );
    }

    #[test]
    fn rva_1ad210_setup_shadow_select_falls_back_or_misses() {
        // C_game::SetupShadow GAME RVA 0x1ad210 (VA 0x5ad210).
        let templates = vec![shadow_template(Some("car")), shadow_template(None)];
        // Nameless item falls back to the model index.
        assert_eq!(
            setup_shadow_select(&templates, None, 1),
            ShadowDuplicateSource::ModelIndex(1)
        );
        // Named item with no match duplicates nothing.
        assert_eq!(
            setup_shadow_select(&templates, Some("house"), 0),
            ShadowDuplicateSource::NoMatch
        );
        assert_eq!(
            setup_shadow_select(&[], Some("car"), 2),
            ShadowDuplicateSource::NoMatch
        );
    }

    #[test]
    fn rva_1acfe0_add_shadow_model_reports_boundary() {
        // C_game::AddShadowModel GAME RVA 0x1acfe0 (VA 0x5acfe0).
        assert_eq!(ADD_SHADOW_MODEL_ENTRY, 0x5acfe0);
        assert_eq!(
            add_shadow_model("car_shadow"),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x5acfe0 })
        );
    }

    struct EntityLog {
        order: Vec<(char, ActorId, bool)>,
    }

    impl EntityBlockBackend for EntityLog {
        fn seat_for_cinema(&mut self, actor: ActorId, unknown: bool) {
            self.order.push(('s', actor, unknown));
        }
        fn wake_for_subtitles(&mut self, actor: ActorId) {
            self.order.push(('w', actor, false));
        }
    }

    fn candidate(actor: ActorId, is_entity: bool) -> BlockCandidate {
        BlockCandidate { actor, is_entity }
    }

    #[test]
    fn rva_1b7ed0_entity_block_seats_entities_active_then_inactive() {
        // C_game::EntityBlock GAME RVA 0x1b7ed0 (VA 0x5b7ed0).
        let active = vec![candidate(1, true), candidate(2, false), candidate(3, true)];
        let inactive = vec![candidate(4, true), candidate(5, false)];
        let mut backend = EntityLog { order: vec![] };
        entity_block(true, true, &active, &inactive, &mut backend);
        // Back of active first, then back of inactive; non-entities skipped.
        assert_eq!(
            backend.order,
            vec![('s', 3, true), ('s', 1, true), ('s', 4, true)]
        );
    }

    #[test]
    fn rva_1b7ed0_entity_block_unblock_wakes_for_subtitles() {
        // C_game::EntityBlock GAME RVA 0x1b7ed0 (VA 0x5b7ed0).
        let active = vec![candidate(1, true)];
        let inactive = vec![candidate(2, true)];
        let mut backend = EntityLog { order: vec![] };
        entity_block(false, true, &active, &inactive, &mut backend);
        assert_eq!(backend.order, vec![('w', 1, false), ('w', 2, false)]);
    }

    struct SoundLog {
        changed: Vec<(ActorId, i32)>,
    }

    impl ActorSoundBackend for SoundLog {
        fn sounds_change(&mut self, actor: ActorId, param: i32) {
            self.changed.push((actor, param));
        }
    }

    #[test]
    fn rva_1b83f0_stop_all_sounds_zeroes_back_first() {
        // C_game::StopAllSounds GAME RVA 0x1b83f0 (VA 0x5b83f0).
        let mut backend = SoundLog { changed: vec![] };
        stop_all_sounds(&[5, 6, 7], &mut backend);
        assert_eq!(backend.changed, vec![(7, 0), (6, 0), (5, 0)]);
        // Empty input drives nothing.
        let mut backend = SoundLog { changed: vec![] };
        stop_all_sounds(&[], &mut backend);
        assert!(backend.changed.is_empty());
    }

    #[test]
    fn rva_1ba310_clear_death_human_fade_empties_and_clears_blur() {
        // C_game::ClearDeathHumanFade GAME RVA 0x1ba310 (VA 0x5ba310).
        let mut fades = vec![DeathHumanFadeEntry { marker: 1 }, DeathHumanFadeEntry { marker: 2 }];
        let mut blur = true;
        clear_death_human_fade(&mut fades, &mut blur);
        assert!(fades.is_empty());
        assert!(!blur);
    }

    struct WeatherLog {
        writes: Vec<(u32, i32)>,
        values: [i32; 2],
    }

    impl WeatherLog {
        fn new(on: i32, count: i32) -> Self {
            Self { writes: vec![], values: [on, count] }
        }
    }

    impl WeatherSystemBackend for WeatherLog {
        fn weather_param(&self, param: u32) -> i32 {
            if param == 0 {
                self.values[0]
            } else {
                self.values[1]
            }
        }
        fn set_weather_param(&mut self, param: u32, value: i32) {
            self.writes.push((param, value));
            if param == 0 {
                self.values[0] = value;
            } else {
                self.values[1] = value;
            }
        }
    }

    #[test]
    fn rva_1b9540_osefuj_pocasi_uninitialized_is_noop() {
        // C_game::OsefujPocasi GAME RVA 0x1b9540 (VA 0x5b9540).
        let mut state = WeatherSwitchState::default();
        let mut backend = WeatherLog::new(1, 80);
        osefuj_pocasi(false, 0, 0, 1, &mut state, &mut backend);
        osefuj_pocasi(true, 0, 0, 1, &mut state, &mut backend);
        assert!(backend.writes.is_empty());
        assert!(!state.weather_on);
    }

    #[test]
    fn rva_1b9540_osefuj_pocasi_read_then_write() {
        // C_game::OsefujPocasi GAME RVA 0x1b9540 (VA 0x5b9540).
        let mut state = WeatherSwitchState { initialized: true, ..WeatherSwitchState::default() };
        let mut backend = WeatherLog::new(1, 80);
        // Read on-switch, then apply the write pass (effects 0: full count).
        state.game_effects = 2;
        state.weather_count = 80;
        osefuj_pocasi(true, 0, 0, 1, &mut state, &mut backend);
        assert!(state.weather_on);
        assert_eq!(backend.writes, vec![(0, 1), (1, 80)]);
        // Read count-switch afterwards.
        backend.values = [0, 40];
        osefuj_pocasi(true, 1, 0, 1, &mut state, &mut backend);
        assert_eq!(state.weather_count, 40);
        // Scene switched off: read updates the switch, write pass skipped.
        backend.values = [0, 40];
        let len = backend.writes.len();
        osefuj_pocasi(true, 0, 0, 1, &mut state, &mut backend);
        assert!(!state.weather_on);
        assert_eq!(backend.writes.len(), len);
    }

    #[test]
    fn rva_1b9540_osefuj_pocasi_write_scales_by_effects() {
        // C_game::OsefujPocasi GAME RVA 0x1b9540 (VA 0x5b9540).
        let mut state = WeatherSwitchState {
            initialized: true,
            weather_on: true,
            weather_count: 81,
            game_effects: 0,
        };
        let mut backend = WeatherLog::new(1, 0);
        osefuj_pocasi(false, u32::MAX, 0, 1, &mut state, &mut backend);
        // 81 / 20 truncates toward zero.
        assert_eq!(backend.writes, vec![(0, 1), (1, 4)]);
        state.game_effects = 1;
        osefuj_pocasi(false, u32::MAX, 0, 1, &mut state, &mut backend);
        assert_eq!(backend.writes.last(), Some(&(1, 10)));
        // Unknown level leaves the scene untouched.
        state.game_effects = 9;
        let len = backend.writes.len();
        osefuj_pocasi(false, u32::MAX, 0, 1, &mut state, &mut backend);
        assert_eq!(backend.writes.len(), len);
        // Weather off: no writes at all.
        state.weather_on = false;
        osefuj_pocasi(false, u32::MAX, 0, 1, &mut state, &mut backend);
        assert_eq!(backend.writes.len(), len);
    }

    struct InterCityLog {
        erased: Vec<u32>,
        released: Vec<FrameId>,
    }

    impl InterCityBackend for InterCityLog {
        fn erase_coll(&mut self, coll: u32) {
            self.erased.push(coll);
        }
        fn release_frame(&mut self, frame: FrameId) {
            self.released.push(frame);
        }
    }

    #[test]
    fn rva_1bbb20_inter_city_game_done_releases_back_first() {
        // C_game::InterCity_GameDoneInGameDone GAME RVA 0x1bbb20 (VA 0x5bbb20).
        let mut boxes = InterCityBoxes {
            colls: Some(vec![Some(10), None, Some(30)]),
            frames: Some(vec![Some(1), Some(2), None]),
            matrices_present: true,
            inv_matrices_present: true,
            num: 3,
        };
        let mut backend = InterCityLog { erased: vec![], released: vec![] };
        inter_city_game_done(&mut boxes, &mut backend);
        // Back first; absent slots skipped.
        assert_eq!(backend.erased, vec![30, 10]);
        assert_eq!(backend.released, vec![2, 1]);
        assert_eq!(boxes.num, 0);
        assert!(boxes.colls.is_none() && boxes.frames.is_none());
        assert!(!boxes.matrices_present && !boxes.inv_matrices_present);
    }

    #[test]
    fn rva_1bbb20_inter_city_game_done_absent_is_noop() {
        // C_game::InterCity_GameDoneInGameDone GAME RVA 0x1bbb20 (VA 0x5bbb20).
        let mut boxes = InterCityBoxes::default();
        let mut backend = InterCityLog { erased: vec![], released: vec![] };
        inter_city_game_done(&mut boxes, &mut backend);
        assert!(backend.erased.is_empty() && backend.released.is_empty());
    }

    #[test]
    fn rva_1bab80_inter_city_box_count_city_gets_four_more() {
        // C_game::InterCity_GameInit GAME RVA 0x1bab80 (VA 0x5bab80), fragment.
        assert_eq!(inter_city_box_count("mise16-mesto"), 13);
        assert_eq!(inter_city_box_count("MISE16-MESTO"), 13);
        assert_eq!(inter_city_box_count("mise01"), 9);
        assert_eq!(inter_city_box_count(""), 9);
    }

    #[test]
    fn rva_1ae480_load_diff_data_reports_boundary() {
        // C_game::LoadDiffData GAME RVA 0x1ae480 (VA 0x5ae480).
        assert_eq!(LOAD_DIFF_DATA_ENTRY, 0x5ae480);
        assert_eq!(
            load_diff_data("diff"),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x5ae480 })
        );
    }

    #[test]
    fn reset_intern_flags_sets_load_and_clears_loaded() {
        // Working-name fragment of the inline-only C_game::ResetIntern
        // (game.h:829-833); no outline GAME entry exists, so no RVA is
        // claimed here.
        let mut flags = InternLoadFlags {
            intern_load: false,
            intern_loaded: true,
        };
        reset_intern_flags(&mut flags);
        assert_eq!(
            flags,
            InternLoadFlags {
                intern_load: true,
                intern_loaded: false,
            }
        );
    }

    #[test]
    fn game_init_score_reset_switches_off_and_zeroes() {
        // Working-name fragment of C_game::Init (game.cpp:269-271); the
        // occupied Init entry keeps its port, so no RVA is claimed here.
        struct Log {
            counts: Vec<i32>,
        }
        impl HudBonusBackend for Log {
            fn bonus_set_count(&mut self, count: i32) {
                self.counts.push(count);
            }
        }
        let mut hud = Log { counts: vec![] };
        let mut state = ScoreState {
            score: 41,
            score_on: true,
        };
        game_init_score_reset(&mut state, &mut hud);
        assert!(!state.score_on);
        assert_eq!(state.score, 0);
        assert_eq!(hud.counts, vec![0x7fffffff]);
    }

    #[test]
    fn game_init_time_clear_zeroes_all_counters() {
        // Working-name fragment of C_game::Init (game.cpp:297-299); no RVA
        // claimed.
        let mut counters = GameTimeCounters {
            game_time: 7,
            game_frames: 9,
            init_frames: 11,
        };
        game_init_time_clear(&mut counters);
        assert_eq!(
            counters,
            GameTimeCounters {
                game_time: 0,
                game_frames: 0,
                init_frames: 0,
            }
        );
    }

    #[test]
    fn game_init_mission_end_clear_resets_end_flags() {
        // Working-name fragment of C_game::Init (game.cpp:294-296); no RVA
        // claimed.
        let mut flags = MissionEndFlags {
            game_done: true,
            mission_end: 2,
            growing_projectors: 5,
        };
        game_init_mission_end_clear(&mut flags);
        assert_eq!(
            flags,
            MissionEndFlags {
                game_done: false,
                mission_end: 0,
                growing_projectors: 0,
            }
        );
    }

    #[test]
    fn game_init_heal_cheat_is_enabled() {
        // Working-name fragment of C_game::Init (game.cpp:273); no RVA
        // claimed.
        assert!(game_init_heal_cheat());
    }

    #[test]
    fn game_init_death_fade_time_matches_init_store() {
        // Working-name fragment of C_game::Init (game.cpp:275); no RVA
        // claimed.
        assert_eq!(GAME_INIT_DEATH_FADE_TIME, 7000);
        assert_eq!(game_init_death_fade_time(), 7000);
    }

    #[test]
    fn game_init_cheats_reset_clears_first_key_and_word() {
        // Working-name fragment of C_game::Init (game.cpp:292-293); no RVA
        // claimed. Only keys[0] is guaranteed by strcpy("", ...).
        let mut state = CheatKeysState {
            keys: [b'X'; 32],
            cheats: 0x1234,
        };
        game_init_cheats_reset(&mut state);
        assert_eq!(state.keys[0], 0);
        assert_eq!(state.keys[1], b'X');
        assert_eq!(state.cheats, 0);
    }

    #[test]
    fn reset_race6_and_timeout_events_nulls_programs_keeps_events() {
        // Working-name fragment of C_game::Init (game.cpp:300-305); no RVA
        // claimed.
        let mut state = GameInlineState::default();
        state.set_timeout_event(11, 21);
        state.set_player_fire_event(12, 22);
        state.set_player_horn_event(13, 23);
        state.set_player_wfall_event(14, 24);
        state.set_race6_state(true);
        reset_race6_and_timeout_events(&mut state);
        assert_eq!(state.timeout_event(), (0, 21));
        assert_eq!(state.player_fire_event(), (0, 22));
        assert_eq!(state.player_horn_event(), (0, 23));
        assert_eq!(state.player_wfall_event(), (0, 24));
        assert_eq!(state.get_race6_state(), RACE6_STATE_NONE);
    }

    #[test]
    fn game_init_weather_buffer_reset_stages_minus_one() {
        // Working-name fragment of C_game::Init (game.cpp:259); no RVA
        // claimed.
        let mut state = GameInlineState::default();
        state.set_weather_buffer(8);
        game_init_weather_buffer_reset(&mut state);
        assert_eq!(state.weather_buffer(), -1);
    }

    #[test]
    fn clear_death_actors_drains_the_list() {
        // Working-name fragment of C_game::Init (game.cpp:306); no RVA
        // claimed.
        let mut actors = vec![3u32, 7u32];
        clear_death_actors(&mut actors);
        assert!(actors.is_empty());
    }

    #[test]
    fn clear_light_cache_drains_and_parks_time() {
        // Working-name fragment of C_game::Init (game.cpp:307-308); no RVA
        // claimed.
        let mut cache = vec![1u32, 2u32];
        let mut time = 41;
        clear_light_cache(&mut cache, &mut time);
        assert!(cache.is_empty());
        assert_eq!(time, -1);
    }

    #[test]
    fn rva_178c60_is_possible_coll_reports_hit_and_miss() {
        // C_human::IsPossibleColl GAME RVA 0x178c60 (VA 0x578c60).
        assert_eq!(is_possible_coll(true, [0.0; 3], [0.0; 3], [0.0; 3], [0.0; 3], 0.0), None);
        let overlap = is_possible_coll(false, [0.0; 3], [0.0; 3], [0.0, 1.0, 0.0], [0.0; 3], 0.0)
            .expect("overlap must report");
        assert_eq!(overlap, (0.0, true));
        let (dist, hit) = is_possible_coll(
            false,
            [0.0; 3],
            [0.0; 3],
            [10.0, 1.0, 0.0],
            [0.0; 3],
            0.0,
        )
        .expect("separated still writes dist");
        assert!(!hit);
        assert_eq!(dist, -1.0);
        let (dist, hit) = is_possible_coll(
            false,
            [1.0, 0.0, 0.0],
            [0.0; 3],
            [5.0, 1.0, 0.0],
            [-1.0, 0.0, 0.0],
            0.0,
        )
        .expect("approach must report");
        assert!(hit);
        assert!(dist > 0.0 && dist < 5.0);
    }

    #[test]
    fn rva_178c60_is_possible_coll_retains_negative_discriminant_and_unbounded_hit() {
        // GAME 0x578c60: the negative discriminant is stored as `dist`
        // rather than the `-1.0` used for zero relative motion.
        let (miss_dist, hit) = is_possible_coll(
            false,
            [0.; 3],
            [0.; 3],
            [4., 1., 0.],
            [0., 0., 1.],
            0.3,
        )
        .unwrap();
        assert!(!hit);
        assert!(miss_dist < -1., "negative discriminant: {miss_dist}");

        // The native method does not clamp its intersection time to a
        // single simulation step. A tangent at t=2 is still a positive hit.
        let (dist, hit) = is_possible_coll(
            false,
            [0.; 3],
            [0.; 3],
            [2., 1., 1.],
            [-1., 0., 0.],
            0.3,
        )
        .unwrap();
        assert!(hit);
        assert_eq!(dist, 2.);
    }

    #[test]
    fn rva_16b180_add_to_cutscene_reports_boundary() {
        // C_human::AddToCutscene GAME RVA 0x16b180 (VA 0x56b180).
        assert_eq!(ADD_TO_CUTSCENE_ENTRY, 0x56b180);
        assert_eq!(
            add_to_cutscene(),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x56b180 })
        );
    }

    #[test]
    fn rva_1b9390_tick_scream_areas_reports_unsupported() {
        // C_game::TickScreamAreas GAME RVA 0x1b9390 (VA 0x5b9390): STUB.
        assert_eq!(TICK_SCREAM_AREAS_ENTRY, 0x5b9390);
        assert_eq!(
            tick_scream_areas(),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x5b9390 })
        );
    }

    #[test]
    fn rva_1baa40_remove_death_human_fade_reports_unsupported() {
        // C_game::RemoveDeathHumanFade GAME RVA 0x1baa40 (VA 0x5baa40): STUB.
        assert_eq!(REMOVE_DEATH_HUMAN_FADE_ENTRY, 0x5baa40);
        assert_eq!(
            remove_death_human_fade(0, false),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x5baa40 })
        );
        assert_eq!(
            remove_death_human_fade(-1, true),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x5baa40 })
        );
    }

    #[test]
    fn rva_1b7f70_add_invisible_actor_reports_unsupported() {
        // C_game::AddInvisibleActor GAME RVA 0x1b7f70 (VA 0x5b7f70): STUB.
        assert_eq!(ADD_INVISIBLE_ACTOR_ENTRY, 0x5b7f70);
        assert_eq!(
            add_invisible_actor(7),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x5b7f70 })
        );
    }

    #[test]
    fn rva_1b8150_find_actors_vect_reports_unsupported() {
        // C_game::FindActorsVect GAME RVA 0x1b8150 (VA 0x5b8150): STUB.
        assert_eq!(FIND_ACTORS_VECT_ENTRY, 0x5b8150);
        assert_eq!(
            find_actors_vect(),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x5b8150 })
        );
    }

    #[test]
    fn rva_1b81e0_add_cutscene_particle_reports_unsupported() {
        // C_game::AddCutsceneParticle GAME RVA 0x1b81e0 (VA 0x5b81e0): STUB.
        assert_eq!(ADD_CUTSCENE_PARTICLE_ENTRY, 0x5b81e0);
        assert_eq!(
            add_cutscene_particle(3),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x5b81e0 })
        );
    }

    #[test]
    fn rva_1b8390_destroy_cutscene_particles_reports_unsupported() {
        // C_game::DestroyCutsceneParticles GAME RVA 0x1b8390 (VA 0x5b8390).
        assert_eq!(DESTROY_CUTSCENE_PARTICLES_ENTRY, 0x5b8390);
        assert_eq!(
            destroy_cutscene_particles(),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x5b8390 })
        );
    }

    #[test]
    fn rva_1b8e70_add_created_frame_to_vector_reports_unsupported() {
        // C_game::AddCreatedFrameToVector GAME RVA 0x1b8e70 (VA 0x5b8e70).
        assert_eq!(ADD_CREATED_FRAME_TO_VECTOR_ENTRY, 0x5b8e70);
        assert_eq!(
            add_created_frame_to_vector(11),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x5b8e70 })
        );
    }

    #[test]
    fn rva_1b9040_del_created_frame_to_vector_returns_false() {
        // C_game::DelCreatedFrameToVector GAME RVA 0x1b9040 (VA 0x5b9040).
        assert!(!del_created_frame_to_vector(11));
        assert!(!del_created_frame_to_vector(0));
    }

    #[test]
    fn rva_1ba010_set_camera_rot_repair_reports_unsupported() {
        // C_game::SetCameraRotRepair GAME RVA 0x1ba010 (VA 0x5ba010): STUB.
        assert_eq!(SET_CAMERA_ROT_REPAIR_ENTRY, 0x5ba010);
        assert_eq!(
            set_camera_rot_repair(),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x5ba010 })
        );
    }

    #[test]
    fn rva_1ba080_clear_cartridges_reports_unsupported() {
        // C_game::ClearCartridges GAME RVA 0x1ba080 (VA 0x5ba080): STUB.
        assert_eq!(CLEAR_CARTRIDGES_ENTRY, 0x5ba080);
        assert_eq!(
            clear_cartridges(),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x5ba080 })
        );
    }

    #[test]
    fn rva_1ba110_tick_car_sound_manager_reports_unsupported() {
        // C_game::TickCarSoundManager GAME RVA 0x1ba110 (VA 0x5ba110): STUB.
        assert_eq!(TICK_CAR_SOUND_MANAGER_ENTRY, 0x5ba110);
        assert_eq!(
            tick_car_sound_manager(),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x5ba110 })
        );
    }

    #[test]
    fn rva_1b5450_lightness_add_reports_unsupported() {
        // C_game::LightnessAdd GAME RVA 0x1b5450 (VA 0x5b5450): STUB.
        assert_eq!(LIGHTNESS_ADD_ENTRY, 0x5b5450);
        assert_eq!(
            lightness_add(1, 2, 3, [0.0; 3]),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x5b5450 })
        );
        assert_eq!(
            lightness_add(u32::MAX, 0, u16::MAX, [1.0, 2.0, 3.0]),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x5b5450 })
        );
    }

    #[test]
    fn rva_1b56f0_lightness_del_reports_unsupported() {
        // C_game::LightnessDel GAME RVA 0x1b56f0 (VA 0x5b56f0): STUB.
        assert_eq!(LIGHTNESS_DEL_ENTRY, 0x5b56f0);
        assert_eq!(
            lightness_del(11),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x5b56f0 })
        );
    }

    #[test]
    fn rva_1b7be0_destroy_control_signal_reports_unsupported() {
        // C_game::DestroyControlSignal GAME RVA 0x1b7be0 (VA 0x5b7be0): STUB.
        assert_eq!(DESTROY_CONTROL_SIGNAL_ENTRY, 0x5b7be0);
        assert_eq!(
            destroy_control_signal(1, [0.0; 3], [0.0; 3], [0.0; 3], 4),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x5b7be0 })
        );
    }

    #[test]
    fn rva_1b9690_add_dead_human_for_tick_reports_unsupported() {
        // C_game::AddDeadHumanForTick GAME RVA 0x1b9690 (VA 0x5b9690): STUB.
        assert_eq!(ADD_DEAD_HUMAN_FOR_TICK_ENTRY, 0x5b9690);
        assert_eq!(
            add_dead_human_for_tick(7),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x5b9690 })
        );
    }

    #[test]
    fn rva_1b98c0_tick_dead_human_for_tick_reports_unsupported() {
        // C_game::TickDeadHumanForTick GAME RVA 0x1b98c0 (VA 0x5b98c0): STUB.
        assert_eq!(TICK_DEAD_HUMAN_FOR_TICK_ENTRY, 0x5b98c0);
        assert_eq!(
            tick_dead_human_for_tick(100),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x5b98c0 })
        );
    }

    #[test]
    fn rva_1ba390_item_blur_tick_reports_unsupported() {
        // C_game::ItemBlurTick GAME RVA 0x1ba390 (VA 0x5ba390): STUB.
        assert_eq!(ITEM_BLUR_TICK_ENTRY, 0x5ba390);
        assert_eq!(
            item_blur_tick(0),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x5ba390 })
        );
        assert_eq!(
            item_blur_tick(u32::MAX),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x5ba390 })
        );
    }

    #[test]
    fn rva_1bab20_rec_prepare_humans_reports_unsupported() {
        // C_game::RecPrepareHumans GAME RVA 0x1bab20 (VA 0x5bab20): STUB.
        assert_eq!(REC_PREPARE_HUMANS_ENTRY, 0x5bab20);
        assert_eq!(
            rec_prepare_humans(),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x5bab20 })
        );
    }

    #[test]
    fn rva_1bb570_add_inter_city_box_reports_unsupported() {
        // AddInterCityBox GAME RVA 0x1bb570 (VA 0x5bb570): STUB.
        assert_eq!(ADD_INTER_CITY_BOX_ENTRY, 0x5bb570);
        assert_eq!(
            add_inter_city_box(),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x5bb570 })
        );
    }

    #[test]
    fn rva_0aa7a0_traffic_car_ctor_reports_unsupported() {
        // C_traffic_car::C_traffic_car GAME RVA 0x0aa7a0 (VA 0x4aa7a0): STUB.
        assert_eq!(TRAFFIC_CAR_CTOR_ENTRY, 0x4aa7a0);
        assert_eq!(
            traffic_car_ctor(),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x4aa7a0 })
        );
    }

    #[test]
    fn rva_0aa830_traffic_car_dtor_reports_unsupported() {
        // C_traffic_car::~C_traffic_car GAME RVA 0x0aa830 (VA 0x4aa830): STUB.
        assert_eq!(TRAFFIC_CAR_DTOR_ENTRY, 0x4aa830);
        assert_eq!(
            traffic_car_dtor(),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x4aa830 })
        );
    }

    #[test]
    fn rva_0aa860_traffic_car_init_reports_unsupported() {
        // C_traffic_car::Init GAME RVA 0x0aa860 (VA 0x4aa860): STUB.
        assert_eq!(TRAFFIC_CAR_INIT_ENTRY, 0x4aa860);
        assert_eq!(
            traffic_car_init(9),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x4aa860 })
        );
        assert_eq!(
            traffic_car_init(0),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x4aa860 })
        );
    }

    #[test]
    fn rva_0aa8b0_traffic_car_change_state_reports_unsupported() {
        // C_traffic_car::ChangeState GAME RVA 0x0aa8b0 (VA 0x4aa8b0): STUB.
        assert_eq!(TRAFFIC_CAR_CHANGE_STATE_ENTRY, 0x4aa8b0);
        assert_eq!(
            traffic_car_change_state(),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x4aa8b0 })
        );
    }

    #[test]
    fn rva_0b54a0_traffic_car_game_init_reports_unsupported() {
        // C_traffic_car::GameInit GAME RVA 0x0b54a0 (VA 0x4b54a0): STUB.
        assert_eq!(TRAFFIC_CAR_GAME_INIT_ENTRY, 0x4b54a0);
        assert_eq!(
            traffic_car_game_init(),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x4b54a0 })
        );
    }

    #[test]
    fn rva_0b8010_traffic_car_game_done_reports_unsupported() {
        // C_traffic_car::GameDone GAME RVA 0x0b8010 (VA 0x4b8010): STUB.
        assert_eq!(TRAFFIC_CAR_GAME_DONE_ENTRY, 0x4b8010);
        assert_eq!(
            traffic_car_game_done(),
            Err(UnwrittenGameEntry::Unsupported { entry: 0x4b8010 })
        );
    }
}
