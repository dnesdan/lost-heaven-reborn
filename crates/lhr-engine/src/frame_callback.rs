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
}
