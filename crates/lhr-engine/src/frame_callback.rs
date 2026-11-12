//! Frame callback slots for LS3DF 0x1b3c0 `I3D_frame::SetCallback`.
//!
//! Original LS3DF SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! Upstream working name only (mafia-re I3d_frame.cpp:223-234); no Ghidra
//! body in session -> tier contract per the 0x2b830 standard.
//!
//! The native callback is a code pointer (`I3D_CALLBACK*`); the host stores
//! an opaque caller token instead and never fabricates a native pointer.
//! Mask 0x1e00 from I3D.h:267 (FRMFLAGS_CALLBACK_MASK). Null clears only
//! masked flag bits and drops the slot (native keeps the stale pointer
//! word; the host keeps no dangling value by construction).

use crate::frame_transform::{FrameError, FrameKey, I3dFrameGraph};
use std::collections::BTreeMap;

/// Opaque caller identity for a native frame callback. Not a host pointer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameCallbackToken(pub u32);

/// Native `FRMFLAGS_CALLBACK_MASK` word (I3D.h:267).
pub const CALLBACK_MASK: u32 = 0x1e00;

/// Native `LS3D_CALLBACK_MESSAGE_0` (mafia-re ls3df/I3D/I3D.h:294-295 at pin
/// 113b5b6). The zero base of the callback message enum; no message selected.
/// The reference records no LS3DF FUNCTION address for the enum values (they
/// are `enum` words, not functions); the values are consumed through
/// `FRMFLAGS_CALLBACK_MASK` by `SetCallback` (PC RVA 0x1b3c0, occupied, not
/// claimed here).
pub const LS3D_CALLBACK_MESSAGE_0: u32 = 0;
/// Native `CBM_PROGRESS` (mafia-re ls3df/I3D/I3D.h:296 at pin 113b5b6;
/// Insanity3D's name): a loader's progress report. Same working-name status
/// as `LS3D_CALLBACK_MESSAGE_0`.
pub const CBM_PROGRESS: u32 = 8;
/// Native `CBM_ERROR` (mafia-re ls3df/I3D/I3D.h:297 at pin 113b5b6;
/// Insanity3D's name). Same working-name status as `LS3D_CALLBACK_MESSAGE_0`.
pub const CBM_ERROR: u32 = 0x10;
/// Native `CBM_LOG` (mafia-re ls3df/I3D/I3D.h:298 at pin 113b5b6;
/// Insanity3D's name). Same working-name status as `LS3D_CALLBACK_MESSAGE_0`.
pub const CBM_LOG: u32 = 0x20;
/// Native `LS3D_CALLBACK_NOTE` (mafia-re ls3df/I3D/I3D.h:299-300 at pin
/// 113b5b6): an animation passed a note key. Same working-name status as
/// `LS3D_CALLBACK_MESSAGE_0`.
pub const LS3D_CALLBACK_NOTE: u32 = 0x200;
/// Native `LS3D_CALLBACK_ANIM_END` (mafia-re ls3df/I3D/I3D.h:302 at pin
/// 113b5b6): an animation reached an end. Same working-name status as
/// `LS3D_CALLBACK_MESSAGE_0`. (`LS3D_CALLBACK_LOD`, 0x400, is intentionally
/// not repeated here: it is owned by `render_lod`, which tests the
/// `GetObjectLOD` dispatch gate against it.)
pub const LS3D_CALLBACK_ANIM_END: u32 = 0x800;
/// Native `LS3D_CALLBACK_NOTE_STRING` (mafia-re ls3df/I3D/I3D.h:303 at pin
/// 113b5b6): an animation passed a string note key. Same working-name status
/// as `LS3D_CALLBACK_MESSAGE_0`.
pub const LS3D_CALLBACK_NOTE_STRING: u32 = 0x1000;

/// Borrowed live view over graph flags plus the owned token registry.
/// The registry lives on the arena owner; this type only carries state.
#[derive(Clone, Debug, Default)]
pub struct FrameCallbacks {
    slots: BTreeMap<u32, FrameCallbackToken>,
}

impl FrameCallbacks {
    pub fn new() -> Self {
        Self::default()
    }

    /// Typed SetCallback (LS3DF RVA 0x1b3c0). Width/truncation: `flags` is
    /// u32, masked with 0x1e00 exactly as the native `flags &= MASK` word
    /// op; no byte narrowing. Effect order: null clears masked bits only
    /// (never stores); non-null stores the token first, then ORs the bits.
    pub fn set_callback(
        &mut self,
        graph: &mut I3dFrameGraph,
        key: FrameKey,
        callback: Option<FrameCallbackToken>,
        flags: u32,
    ) -> Result<(), FrameError> {
        let masked = flags & CALLBACK_MASK;
        // Live read before mutation (view discipline).
        graph.state(key)?;
        match callback {
            None => {
                graph.change_flags(key, masked, 0)?;
                self.slots.remove(&key.0);
            }
            Some(token) => {
                self.slots.insert(key.0, token);
                graph.change_flags(key, 0, masked)?;
            }
        }
        Ok(())
    }

    /// Current token for a frame, if any.
    pub fn get(&self, key: FrameKey) -> Option<FrameCallbackToken> {
        self.slots.get(&key.0).copied()
    }

    /// Duplicate-time slot copy (LS3DF RVA 0x1c5b0 step 7): copies the token
    /// without touching flags (flag words belong to the field-mask step).
    /// Absent stays absent; no stale residue by construction.
    pub(crate) fn copy_slot(&mut self, destination: FrameKey, source: FrameKey) {
        if let Some(token) = self.slots.get(&source.0).copied() {
            self.slots.insert(destination.0, token);
        } else {
            self.slots.remove(&destination.0);
        }
    }

    /// Destroy-tail slot drop: frames leaving the arena must not leak
    /// registry entries (keys are never reused, so this is a leak, never
    /// a wrong-slot read; still, the destroy tail owns it).
    pub(crate) fn remove_slot(&mut self, key: FrameKey) {
        self.slots.remove(&key.0);
    }

    /// Test-only slot presence probe (the public getter liveness-gates,
    /// so it cannot observe leaked entries on dead frames).
    #[cfg(test)]
    pub(crate) fn has_slot(&self, key: FrameKey) -> bool {
        self.slots.contains_key(&key.0)
    }

    /// Working-name presence probe for a stored callback token (mafia-re
    /// ls3df/I3D/I3D.h:313-314 callback slot; no LS3DF FUNCTION address for a
    /// standalone reader). Reports whether a token is registered; no flag
    /// moves.
    pub fn has_callback(&self, key: FrameKey) -> bool {
        self.slots.contains_key(&key.0)
    }
}

/// Working name for the FRMFLAGS_CALLBACK_MASK word op (mafia-re
/// ls3df/I3D/I3D.h:267; applied by I3D_frame::SetCallback at
/// ls3df/I3D/frames/I3d_frame.cpp:223-234). The reference records no LS3DF
/// FUNCTION address for the mask itself. Applies the evidenced 0x1e00 mask
/// with u32 width and no byte narrowing; pure helper, no function credit
/// beyond the SetCallback contract.
pub fn callback_masked_flags(flags: u32) -> u32 {
    flags & CALLBACK_MASK
}

/// Working name for the `flags & message` dispatch gate behind the callback
/// message words (mafia-re ls3df/I3D/I3D.h:293-304 at pin 113b5b6). The
/// reference records no LS3DF FUNCTION address for a standalone gate; callers
/// test the stored flags before invoking the callback (for example,
/// `GetObjectLOD` tests `frm_flags & LS3D_CALLBACK_LOD`, owned by
/// `render_lod`). Reports whether `message` is selected in `flags`; pure word
/// op, no registry or flag moves.
pub fn callback_message_enabled(flags: u32, message: u32) -> bool {
    flags & message != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callback_mask_keeps_only_the_message_bits() {
        // Working name for the I3D.h:267 mask word: no LS3DF FUNCTION address.
        assert_eq!(callback_masked_flags(0xffff_ffff), CALLBACK_MASK);
        assert_eq!(callback_masked_flags(0x1e00), 0x1e00);
        assert_eq!(callback_masked_flags(0x01ff), 0);
        assert_eq!(callback_masked_flags(0), 0);
    }

    #[test]
    fn callback_presence_probe_tracks_slot_without_flag_moves() {
        // Uses only this module's registry; the graph flag mock is local.
        let mut callbacks = FrameCallbacks::new();
        let key = FrameKey(7);
        assert!(!callbacks.has_callback(key));
        callbacks.slots.insert(key.0, FrameCallbackToken(3));
        assert!(callbacks.has_callback(key));
        assert_eq!(callbacks.get(key), Some(FrameCallbackToken(3)));
        callbacks.slots.remove(&key.0);
        assert!(!callbacks.has_callback(key));
    }

    #[test]
    fn callback_message_words_match_ls3df_dispatch_values() {
        // Working names for the LS3D_CALLBACK_MESSAGE enum words (mafia-re
        // ls3df/I3D/I3D.h:294-303): the reference records no LS3DF FUNCTION
        // addresses (enum words, not functions). LOD (0x400) is owned by
        // render_lod and is not repeated here.
        assert_eq!(LS3D_CALLBACK_MESSAGE_0, 0);
        assert_eq!(CBM_PROGRESS, 8);
        assert_eq!(CBM_ERROR, 0x10);
        assert_eq!(CBM_LOG, 0x20);
        assert_eq!(LS3D_CALLBACK_NOTE, 0x200);
        assert_eq!(LS3D_CALLBACK_ANIM_END, 0x800);
        assert_eq!(LS3D_CALLBACK_NOTE_STRING, 0x1000);
        assert_eq!(
            CBM_PROGRESS
                | CBM_ERROR
                | CBM_LOG
                | LS3D_CALLBACK_NOTE
                | LS3D_CALLBACK_ANIM_END
                | LS3D_CALLBACK_NOTE_STRING,
            0x1a38
        );
    }

    #[test]
    fn callback_message_enabled_reports_only_selected_bits() {
        assert!(callback_message_enabled(
            LS3D_CALLBACK_NOTE,
            LS3D_CALLBACK_NOTE
        ));
        assert!(callback_message_enabled(u32::MAX, CBM_LOG));
        assert!(!callback_message_enabled(0, CBM_ERROR));
        assert!(!callback_message_enabled(CBM_PROGRESS, LS3D_CALLBACK_NOTE));
        assert!(!callback_message_enabled(
            u32::MAX & !CBM_PROGRESS,
            CBM_PROGRESS
        ));
    }
}
