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
}
