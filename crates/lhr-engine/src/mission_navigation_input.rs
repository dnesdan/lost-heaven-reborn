//! Installation-backed input for the existing owned C_web_path loader.
//!
//! Host composition, not a second graph or a complete C_mission::Open. The
//! caller supplies the actual mission's WebPath component and allocator. Use
//! the existing Installation resolver (including selected-archive precedence)
//! rather than opening a loose file independently.
//!
//! PC C_mission::Open passes a freshly opened C_chunk directly to LoadNodes,
//! without RAscend. Its root stack is empty, unlike PS2's initial file-size
//! entry. Consequently reads can return a short prefix, which the existing
//! LoadNodes implementation handles in its evidenced order. A synthetic EOF
//! chunk bound or read_exact would change that behavior.
//!
//! Reference: mafia-re dc867c3238488de27d094bb79b6550e392566e3c,
//! game/C_chunk.cpp, game/C_chunk.h and game/mission/Mission.cpp. The existing
//! loader has separate PC evidence; no new original-function credit is claimed.
#![forbid(unsafe_code)]

use crate::mission_components::{
    MissionComponent, WebPathAllocator, WebPathChunk, WebPathLoadError,
};
use lhr_assets::Installation;
use std::convert::Infallible;

/// Read-only host diagnostics, not another copy of navigation state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NavigationReadReceipt {
    pub source_bytes: usize,
    pub consumed_bytes: usize,
    pub read_calls: usize,
    pub short_reads: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationLoadOutcome {
    /// No selected archive or loose entry exists. The graph was not touched.
    Absent,
    /// LoadNodes returned normally. This does NOT assert structural validity:
    /// the original also returns for a mismatched signature and ignores some
    /// short reads. Inspect the receipt and the same component's live graph.
    Returned(NavigationReadReceipt),
}

#[derive(Debug)]
pub enum NavigationInputError<E> {
    /// Resolver, containment, selected-source, quota or I/O errors are not absence.
    Source(lhr_assets::Error),
    /// The private reader's signed position domain; Installation currently has
    /// a stricter 64 MiB payload quota. This is a host admission, not a file tag.
    SourceExtent { bytes: usize },
    Load {
        error: WebPathLoadError<E, Infallible>,
        read: NavigationReadReceipt,
    },
}

/// Read a resolved check.bin into the caller's existing navigation owner.
///
/// `relative_path` is the path selected by the mission orchestration; it is
/// validated by Installation. This function does not duplicate mission-name
/// policy, create a mission/component, reset flags, or advance GameInit.
///
/// Source resolution/admission completes before LoadNodes enters. The existing
/// Installation API buffers payloads, so this is not an emulation of late disk
/// or archive I/O faults. Resolver failures leave the old graph untouched.
/// Once LoadNodes enters, its destruction, allocation, partial writes and
/// interrupted-owner guard remain authoritative; there is no rollback/retry.
/// A missing optional file skips even component/allocator validation, matching
/// the caller's ROpen-failed branch rather than clearing an existing graph.
pub fn load_installed_navigation<A: WebPathAllocator>(
    component: &mut MissionComponent,
    allocator: &mut A,
    installation: &Installation,
    relative_path: &str,
) -> Result<NavigationLoadOutcome, NavigationInputError<A::Error>> {
    let Some(bytes) = installation
        .read_optional(relative_path)
        .map_err(NavigationInputError::Source)?
    else {
        return Ok(NavigationLoadOutcome::Absent);
    };
    if bytes.len() > i32::MAX as usize {
        return Err(NavigationInputError::SourceExtent { bytes: bytes.len() });
    }
    let mut input = RootNavigationInput {
        bytes: &bytes,
        position: 0,
        read_calls: 0,
        short_reads: 0,
    };
    let result = component.load_web_nodes(allocator, &mut input);
    let read = input.receipt();
    result.map_err(|error| NavigationInputError::Load { error, read })?;
    Ok(NavigationLoadOutcome::Returned(read))
}

/// Borrowed file payload only. No chunk has been ascended and therefore there
/// is no active bound cell to synthesize or copy. The outer call admits a signed
/// 32-bit extent; position starts at zero and only advances within these bytes.
struct RootNavigationInput<'a> {
    bytes: &'a [u8],
    position: usize,
    read_calls: usize,
    short_reads: usize,
}
impl RootNavigationInput<'_> {
    fn receipt(&self) -> NavigationReadReceipt {
        NavigationReadReceipt {
            source_bytes: self.bytes.len(),
            consumed_bytes: self.position,
            read_calls: self.read_calls,
            short_reads: self.short_reads,
        }
    }
}
impl WebPathChunk for RootNavigationInput<'_> {
    type Error = Infallible;
    type Bound = Infallible;

    fn is_open(&self) -> bool {
        true
    }
    fn is_writing(&self) -> bool {
        false
    }
    fn active_bound(&self) -> Option<Self::Bound> {
        None
    }
    fn seek_current(&mut self) -> Result<i32, Self::Error> {
        // The private constructor admits source length <= i32::MAX.
        Ok(self.position as i32)
    }
    fn read_bound(&self, bound: Self::Bound) -> Result<i32, Self::Error> {
        match bound {}
    }
    fn read_data(&mut self, destination: &mut [u8]) -> Result<usize, Self::Error> {
        let count = destination.len().min(self.bytes.len() - self.position);
        destination[..count].copy_from_slice(&self.bytes[self.position..self.position + count]);
        self.position += count;
        self.read_calls = self.read_calls.saturating_add(1);
        if count < destination.len() {
            self.short_reads = self.short_reads.saturating_add(1);
        }
        // Never zero or otherwise write the unread destination suffix.
        Ok(count)
    }
}

#[cfg(test)]
#[path = "mission_navigation_input_tests.rs"]
mod tests;
