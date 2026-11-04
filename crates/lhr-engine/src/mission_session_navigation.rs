//! Session-owned `check.bin` navigation intake in `C_mission::Open` order.
//!
//! Reference: mafia-re `d0fbd6d`, game/mission/Mission.cpp, `C_mission::Open`
//! (PC 0x5409d0): `check.bin` loads after sector-group fill and before
//! `road.bin`/`OpenBin`. The graph loader itself (`WebPathGraph::load`,
//! PC 0x14a8c0) is the existing evidenced owner; this module supplies the
//! session allocator and lifecycle (construct, load once, inspect, destroy)
//! so `MissionLoadSession` can own navigation instead of diagnosing it from
//! the side.
//!
//! Absence is the original `ROpen`-failed branch: no component is
//! constructed and the load continues. Any other source, allocator, or graph
//! failure stops the load with its first error; an interrupted component is
//! never retried.
#![forbid(unsafe_code)]

use crate::mission_components::{
    MissionAllocation, MissionAllocator, MissionComponent, MissionComponentKind,
    MissionMemoryDomain, MissionMemoryDomainId, WebArrayKind, WebPathAllocator,
};
use crate::mission_navigation_input::{load_installed_navigation, NavigationLoadOutcome};
use std::collections::BTreeMap;

/// Host admission for the session navigation domain.
const MAX_LIVE_BYTES: usize = 64 * 1024 * 1024;
const MAX_LIVE_ALLOCATIONS: usize = 1_048_576;
const IDENTITY_FIRST: u32 = 0x10000;

/// Allocator failures stay host-side; no native pointer is invented.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionNavAllocError {
    ByteQuotaExceeded { requested: usize, live: usize },
    AllocationQuotaExceeded { live: usize },
    UnownedFree { identity: u32 },
    EffectsTableUnreached,
}

/// Bounded session allocator for one navigation component and its arrays.
/// Identities are host-issued nonzero tokens, never native addresses.
#[derive(Debug)]
pub struct SessionNavigationAllocator {
    domain: MissionMemoryDomainId,
    next_identity: u32,
    live: BTreeMap<u32, Vec<u8>>,
    live_bytes: usize,
}

impl SessionNavigationAllocator {
    fn new() -> Self {
        Self {
            domain: MissionMemoryDomainId::fresh(),
            next_identity: IDENTITY_FIRST,
            live: BTreeMap::new(),
            live_bytes: 0,
        }
    }

    fn issue(&mut self, bytes: usize) -> Result<MissionAllocation, SessionNavAllocError> {
        if self.live.len() >= MAX_LIVE_ALLOCATIONS {
            return Err(SessionNavAllocError::AllocationQuotaExceeded {
                live: self.live.len(),
            });
        }
        self.live_bytes
            .checked_add(bytes)
            .filter(|total| *total <= MAX_LIVE_BYTES)
            .ok_or(SessionNavAllocError::ByteQuotaExceeded {
                requested: bytes,
                live: self.live_bytes,
            })?;
        let identity = self.next_identity;
        self.next_identity = self.next_identity.checked_add(1).ok_or(
            SessionNavAllocError::AllocationQuotaExceeded {
                live: self.live.len(),
            },
        )?;
        if identity == 0 {
            return Err(SessionNavAllocError::AllocationQuotaExceeded {
                live: self.live.len(),
            });
        }
        self.live.insert(identity, vec![0u8; bytes]);
        self.live_bytes += bytes;
        Ok(MissionAllocation {
            domain: self.domain,
            identity,
            bytes: vec![0u8; bytes],
        })
    }

    /// Live allocation count for ownership diagnostics.
    pub fn live_allocations(&self) -> usize {
        self.live.len()
    }

    /// Live byte count for ownership diagnostics.
    pub fn live_bytes(&self) -> usize {
        self.live_bytes
    }
}

impl MissionMemoryDomain for SessionNavigationAllocator {
    fn mission_memory_domain(&self) -> MissionMemoryDomainId {
        self.domain
    }
}

impl MissionAllocator for SessionNavigationAllocator {
    type Error = SessionNavAllocError;

    fn allocate(
        &mut self,
        kind: MissionComponentKind,
    ) -> Result<Option<MissionAllocation>, Self::Error> {
        Ok(Some(self.issue(kind.allocation_size())?))
    }

    fn free(&mut self, identity: u32) -> Result<(), Self::Error> {
        if identity == 0 {
            return Ok(());
        }
        let bytes = self
            .live
            .remove(&identity)
            .ok_or(SessionNavAllocError::UnownedFree { identity })?;
        self.live_bytes -= bytes.len();
        Ok(())
    }

    fn open_effects_table(&mut self) -> Result<bool, Self::Error> {
        Err(SessionNavAllocError::EffectsTableUnreached)
    }
}

impl WebPathAllocator for SessionNavigationAllocator {
    fn allocate_web_array(
        &mut self,
        _kind: WebArrayKind,
        _elements: u32,
        byte_length: u32,
    ) -> Result<Option<MissionAllocation>, Self::Error> {
        Ok(Some(self.issue(byte_length as usize)?))
    }
}

/// Terminal outcome of the single navigation load attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionNavigationOutcome {
    NotAttempted,
    Absent {
        path: String,
    },
    Loaded {
        path: String,
        source_bytes: usize,
        nodes: u32,
        links: u32,
        read_calls: usize,
        short_reads: usize,
    },
    SourceUnavailable {
        path: String,
        detail: String,
    },
    Failed {
        path: String,
        detail: String,
    },
}

/// Load-stopping failure; absence is an outcome, not an error here.
#[derive(Debug)]
pub enum SessionNavigationCause {
    AlreadyAttempted,
    Allocator(SessionNavAllocError),
    Component(crate::mission_components::ComponentError<SessionNavAllocError>),
    Load(String),
    Source(String),
}

/// Session-owned navigation lifecycle: construct once, load once, inspect,
/// destroy explicitly. A failed load keeps its interrupted component without
/// retry; destruction still runs to release the retained prefix.
pub struct SessionNavigation {
    allocator: SessionNavigationAllocator,
    component: Option<MissionComponent>,
    outcome: SessionNavigationOutcome,
}

impl std::fmt::Debug for SessionNavigation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionNavigation")
            .field("outcome", &self.outcome)
            .field("live_allocations", &self.allocator.live_allocations())
            .field("live_bytes", &self.allocator.live_bytes())
            .field("component_present", &self.component.is_some())
            .finish()
    }
}

impl SessionNavigation {
    pub fn new() -> Self {
        Self {
            allocator: SessionNavigationAllocator::new(),
            component: None,
            outcome: SessionNavigationOutcome::NotAttempted,
        }
    }

    pub fn outcome(&self) -> &SessionNavigationOutcome {
        &self.outcome
    }

    /// Live graph node count; zero unless loaded and not destroyed.
    pub fn node_count(&self) -> u32 {
        self.component
            .as_ref()
            .and_then(|component| component.web_path())
            .map_or(0, |view| view.node_count())
    }

    /// Total connection count across live nodes; zero unless loaded.
    pub fn link_count(&self) -> u32 {
        let Some(component) = self.component.as_ref() else {
            return 0;
        };
        let Some(view) = component.web_path() else {
            return 0;
        };
        let mut total = 0u32;
        for index in 0..view.node_count() as usize {
            if let Some(node) = view.node(index) {
                total = total.saturating_add(u32::from(node.connection_count()));
            }
        }
        total
    }

    pub fn live_allocations(&self) -> usize {
        self.allocator.live_allocations()
    }

    pub fn live_bytes(&self) -> usize {
        self.allocator.live_bytes()
    }

    /// Single navigation load attempt in `Open` order. Absence constructs
    /// nothing and succeeds; every other failure stops the caller.
    pub fn load(
        &mut self,
        installation: &lhr_assets::Installation,
        relative_path: &str,
    ) -> Result<(), SessionNavigationCause> {
        if !matches!(self.outcome, SessionNavigationOutcome::NotAttempted) {
            return Err(SessionNavigationCause::AlreadyAttempted);
        }
        let path = relative_path.to_owned();
        let bytes = installation.read_optional(relative_path).map_err(|error| {
            self.outcome = SessionNavigationOutcome::SourceUnavailable {
                path: path.clone(),
                detail: error.to_string(),
            };
            SessionNavigationCause::Source(error.to_string())
        })?;
        let Some(source_len) = bytes.as_ref().map(Vec::len) else {
            self.outcome = SessionNavigationOutcome::Absent { path };
            return Ok(());
        };
        let allocation = self
            .allocator
            .allocate(MissionComponentKind::WebPath)
            .map_err(|error| {
                self.outcome = SessionNavigationOutcome::Failed {
                    path: path.clone(),
                    detail: format!("component allocation: {error:?}"),
                };
                SessionNavigationCause::Allocator(error)
            })?
            .ok_or_else(|| {
                self.outcome = SessionNavigationOutcome::Failed {
                    path: path.clone(),
                    detail: "component allocation returned null".into(),
                };
                SessionNavigationCause::Load("component allocation returned null".into())
            })?;
        let mut component = MissionComponent::construct(MissionComponentKind::WebPath, allocation)
            .map_err(|error| {
                // The constructor error carries no allocator failure (its
                // parameter is `Infallible`); an invalid allocation here is a
                // host bug, reported without inventing allocator state.
                let detail = format!("component construction: {error:?}");
                self.outcome = SessionNavigationOutcome::Failed {
                    path: path.clone(),
                    detail: detail.clone(),
                };
                SessionNavigationCause::Load(detail)
            })?;
        let outcome = load_installed_navigation(
            &mut component,
            &mut self.allocator,
            installation,
            relative_path,
        );
        match outcome {
            Ok(NavigationLoadOutcome::Returned(read)) => {
                let nodes = component.web_path().map_or(0, |view| view.node_count());
                let mut links = 0u32;
                if let Some(view) = component.web_path() {
                    for index in 0..view.node_count() as usize {
                        if let Some(node) = view.node(index) {
                            links = links.saturating_add(u32::from(node.connection_count()));
                        }
                    }
                }
                self.outcome = SessionNavigationOutcome::Loaded {
                    path,
                    source_bytes: source_len,
                    nodes,
                    links,
                    read_calls: read.read_calls,
                    short_reads: read.short_reads,
                };
                self.component = Some(component);
                Ok(())
            }
            Ok(NavigationLoadOutcome::Absent) => {
                // The file vanished between the presence check and the load;
                // treat it as the absent branch without retaining a component.
                let identity = component.identity();
                self.allocator
                    .free(identity)
                    .map_err(SessionNavigationCause::Allocator)?;
                self.outcome = SessionNavigationOutcome::Absent { path };
                Ok(())
            }
            Err(error) => {
                let detail = format!("{error:?}");
                self.outcome = SessionNavigationOutcome::Failed {
                    path: path.clone(),
                    detail: detail.clone(),
                };
                // The interrupted component stays owned so terminal disposal
                // can release its retained prefix; no retry is admitted.
                self.component = Some(component);
                Err(SessionNavigationCause::Load(detail))
            }
        }
    }

    /// Explicit teardown: destroy the component, free its identity, and
    /// require an empty allocator. Absent/not-attempted loads succeed empty.
    pub fn destroy(&mut self) -> Result<(), SessionNavigationCause> {
        if let Some(component) = self.component.as_mut() {
            let identity = component.identity();
            component
                .destroy(&mut self.allocator)
                .map_err(SessionNavigationCause::Component)?;
            self.allocator
                .free(identity)
                .map_err(SessionNavigationCause::Allocator)?;
        }
        self.component = None;
        if self.allocator.live_allocations() != 0 {
            return Err(SessionNavigationCause::Load(format!(
                "navigation allocator retains {} live allocations",
                self.allocator.live_allocations()
            )));
        }
        Ok(())
    }
}

impl Default for SessionNavigation {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "lhr-session-nav-{}-{}",
                std::process::id(),
                crate::frame_lookup::incarnation()
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn write(&self, name: &str, bytes: &[u8]) {
            std::fs::write(self.0.join(name), bytes).unwrap();
        }
        fn installation(&self) -> lhr_assets::Installation {
            lhr_assets::Installation::open(&self.0).unwrap()
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn node(connections: u8) -> [u8; 30] {
        let mut bytes = [0x77u8; 30];
        bytes[24..28].copy_from_slice(&0xf123_4567u32.to_le_bytes());
        bytes[28] = 200;
        bytes[29] = connections;
        bytes
    }

    fn payload(nodes: &[[u8; 30]], links: &[[u8; 8]]) -> Vec<u8> {
        let mut bytes = 0x01ab_cedfu32.to_le_bytes().to_vec();
        bytes.extend_from_slice(&(nodes.len() as u32).to_le_bytes());
        for node in nodes {
            bytes.extend_from_slice(node);
        }
        for link in links {
            bytes.extend_from_slice(link);
        }
        bytes
    }

    #[test]
    fn absent_file_constructs_nothing_and_succeeds() {
        let scratch = Scratch::new();
        let mut nav = SessionNavigation::new();
        nav.load(&scratch.installation(), "check.bin").unwrap();
        assert_eq!(
            nav.outcome(),
            &SessionNavigationOutcome::Absent {
                path: "check.bin".into()
            }
        );
        assert_eq!(nav.node_count(), 0);
        assert_eq!(nav.link_count(), 0);
        assert_eq!(nav.live_allocations(), 0);
        nav.destroy().unwrap();
    }

    #[test]
    fn loaded_graph_reports_nodes_and_links() {
        let scratch = Scratch::new();
        let link = [3u8, 0, 0, 0, 0x11, 0x22, 0x33, 0x44];
        scratch.write("check.bin", &payload(&[node(1), node(0)], &[link]));
        let mut nav = SessionNavigation::new();
        nav.load(&scratch.installation(), "check.bin").unwrap();
        match nav.outcome() {
            SessionNavigationOutcome::Loaded { nodes, links, .. } => {
                assert_eq!(*nodes, 2);
                assert_eq!(*links, 1);
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
        assert_eq!(nav.node_count(), 2);
        assert_eq!(nav.link_count(), 1);
        assert!(nav.live_allocations() > 0);
        nav.destroy().unwrap();
        assert_eq!(nav.live_allocations(), 0);
        assert_eq!(nav.live_bytes(), 0);
        assert_eq!(nav.node_count(), 0);
    }

    #[test]
    fn second_load_is_rejected_without_touching_state() {
        let scratch = Scratch::new();
        let mut nav = SessionNavigation::new();
        nav.load(&scratch.installation(), "check.bin").unwrap();
        let error = nav.load(&scratch.installation(), "check.bin").unwrap_err();
        assert!(matches!(error, SessionNavigationCause::AlreadyAttempted));
        nav.destroy().unwrap();
    }

    #[test]
    fn truncated_signature_fails_and_retains_prefix_for_disposal() {
        // A short signature prefix leaves unknown bytes at their first use;
        // the loader stops after its disposal prefix (existing LoadNodes rule).
        let scratch = Scratch::new();
        scratch.write("check.bin", &0x01ab_cedfu32.to_le_bytes()[..2]);
        let mut nav = SessionNavigation::new();
        let error = nav.load(&scratch.installation(), "check.bin").unwrap_err();
        assert!(matches!(error, SessionNavigationCause::Load(_)));
        assert!(matches!(
            nav.outcome(),
            SessionNavigationOutcome::Failed { .. }
        ));
        // The interrupted component refuses teardown; the disposal boundary
        // stays explicit instead of freeing a retained prefix implicitly.
        assert!(nav.destroy().is_err());
    }
}
