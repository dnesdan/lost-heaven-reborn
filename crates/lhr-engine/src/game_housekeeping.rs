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
}
