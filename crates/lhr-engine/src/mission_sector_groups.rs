//! Mission world setup over the existing frame and lighting owners.
//! Source: mafia-re dc867c3238488de27d094bb79b6550e392566e3c,
//! game/mission/Mission.cpp cbSectorBitFill (GAME 0x540ff0), and
//! ls3df/I3D/I3D.h I3D_sector::SetGroupBits (LS3DF 0x100508b0).
//! These source correspondences are not original-executable validation.
#![forbid(unsafe_code)]

use crate::frame_sector::SectorError;
use crate::frame_transform::{FrameKey, FrameKind};
use crate::model_loading::{ModelArena, ModelError};
use crate::scene_ownership::{SceneKey, SectorKey};
use std::collections::HashSet;

impl ModelArena {
    /// SetGroupBits writes the mask and invalidates lighting even if unchanged.
    /// Use the same per-sector generation that light registration/setters use.
    pub(crate) fn set_sector_group_bits(
        &mut self,
        sector: SectorKey,
        bits: u32,
    ) -> Result<(), ModelError> {
        self.check_scene_frame_available(sector.frame())?;
        let index = self.sector_index(sector)?;
        let geometry = &mut self.sectors[index].geometry;
        if geometry.destructor_entered || geometry.update_interrupted {
            return Err(SectorError::Interrupted.into());
        }
        geometry.group_bits = bits;
        let changed = self
            .graph
            .sector_light_changes
            .entry(sector.frame().0)
            .or_default();
        *changed = changed.wrapping_add(1);
        Ok(())
    }
}

/// The original starts at Primary, not the scene's membership/sector lists.
/// Each sector changes before its children, visited in descending child index.
/// This is called once by MissionLoadSession after world intake and before scene2.
/// Its existing single-use guard retains any reached error prefix; no rollback
/// or replay is introduced here. Other MissionOpen dependencies remain separate.
pub(super) fn fill_primary_sector_groups(
    arena: &mut ModelArena,
    scene: SceneKey,
) -> Result<(), ModelError> {
    arena.check_owner_available(scene.into())?;
    let primary = arena.scene(scene)?.sectors[0].frame();
    // These operations cannot create/remove frames; this is only a host work bound.
    let maximum_frames = arena.live_frame_count();
    let mut pending = Vec::new();
    let mut seen = HashSet::new();
    enter(arena, primary, maximum_frames, &mut pending, &mut seen)?;
    while let Some(level) = pending.last_mut() {
        if level.remaining == 0 {
            pending.pop();
            continue;
        }
        level.remaining -= 1;
        // Read the live child identity at the same loop iteration as GetChild.
        let child = child_at(arena, level.parent, level.remaining)?;
        enter(arena, child, maximum_frames, &mut pending, &mut seen)?;
    }
    Ok(())
}

// Traversal continuation only: no copies of sector state or resource ownership.
struct Children {
    parent: FrameKey,
    remaining: u32,
}

fn enter(
    arena: &mut ModelArena,
    frame: FrameKey,
    maximum_frames: usize,
    pending: &mut Vec<Children>,
    seen: &mut HashSet<u32>,
) -> Result<(), ModelError> {
    arena.check_scene_frame_available(frame)?;
    seen.try_reserve(1)
        .map_err(|_| ModelError::AllocationOverflow)?;
    if !seen.insert(frame.0) {
        return Err(ModelError::OwnershipCycle);
    }
    if arena.state(frame)?.kind == FrameKind::Sector {
        arena.set_sector_group_bits(SectorKey(frame), u32::MAX)?;
    }
    let remaining = arena.links(frame)?.child_count;
    // Host guard for corrupt topology, not an original engine limit. It also
    // bounds each indexed walk without assuming a native pointer is readable.
    if remaining as usize > maximum_frames {
        return Err(ModelError::OwnershipCycle);
    }
    if remaining != 0 {
        pending
            .try_reserve(1)
            .map_err(|_| ModelError::AllocationOverflow)?;
        pending.push(Children {
            parent: frame,
            remaining,
        });
    }
    Ok(())
}

fn child_at(arena: &ModelArena, parent: FrameKey, index: u32) -> Result<FrameKey, ModelError> {
    let mut child = arena.links(parent)?.child;
    for _ in 0..index {
        let current = child.ok_or(ModelError::OwnershipCycle)?;
        child = arena.links(current)?.next;
    }
    child.ok_or(ModelError::OwnershipCycle)
}

#[cfg(test)]
#[path = "mission_sector_groups_tests.rs"]
mod tests;
