//! Session-owned `road.bin` network intake in `C_mission::Open` order.
//!
//! Reference: mafia-re `d0fbd6d`, game/mission/Mission.cpp, `C_mission::Open`
//! (PC 0x5409d0): `road.bin` loads after `check.bin` and before `OpenBin`.
//! The byte format is established by REA Ghidra analysis of
//! `C_roads::LoadData` (PC 0x559e60; see `road_network.rs`); this module
//! supplies the session lifecycle (load once, inspect, destroy) so
//! `MissionLoadSession` owns the network instead of diagnosing it.
//!
//! Absence is the original `ROpen`-failed branch: nothing is retained and
//! the load continues. Any other source or parse failure stops the load.
//! Parsing is all-or-nothing, so a failure retains no partial graph.
#![forbid(unsafe_code)]

use crate::road_network::{parse_road_network, RoadNetwork};

/// Terminal outcome of the single roads load attempt.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum SessionRoadsOutcome {
    #[default]
    NotAttempted,
    Absent {
        path: String,
    },
    Loaded {
        path: String,
        source_bytes: usize,
        nodes: usize,
        links: usize,
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
pub enum SessionRoadsCause {
    AlreadyAttempted,
    Source(String),
    Parse(String),
}

/// Session-owned road network lifecycle: load once, inspect, destroy
/// explicitly. The network is plain Rust storage; dropping it releases
/// everything, but the explicit destroy keeps the ownership audit exact.
#[derive(Debug, Default)]
pub struct SessionRoads {
    network: Option<RoadNetwork>,
    outcome: SessionRoadsOutcome,
}

impl SessionRoads {
    pub fn new() -> Self {
        Self {
            network: None,
            outcome: SessionRoadsOutcome::NotAttempted,
        }
    }

    pub fn outcome(&self) -> &SessionRoadsOutcome {
        &self.outcome
    }

    pub fn network(&self) -> Option<&RoadNetwork> {
        self.network.as_ref()
    }

    /// Live node count; zero unless loaded and not destroyed.
    pub fn node_count(&self) -> usize {
        self.network
            .as_ref()
            .map_or(0, |network| network.nodes.len())
    }

    /// Live link count; zero unless loaded and not destroyed.
    pub fn link_count(&self) -> usize {
        self.network
            .as_ref()
            .map_or(0, |network| network.links.len())
    }

    /// Single roads load attempt in `Open` order. Absence retains nothing
    /// and succeeds; every other failure stops the caller.
    pub fn load(
        &mut self,
        installation: &lhr_assets::Installation,
        relative_path: &str,
    ) -> Result<(), SessionRoadsCause> {
        if !matches!(self.outcome, SessionRoadsOutcome::NotAttempted) {
            return Err(SessionRoadsCause::AlreadyAttempted);
        }
        let path = relative_path.to_owned();
        let bytes = installation.read_optional(relative_path).map_err(|error| {
            self.outcome = SessionRoadsOutcome::SourceUnavailable {
                path: path.clone(),
                detail: error.to_string(),
            };
            SessionRoadsCause::Source(error.to_string())
        })?;
        let Some(bytes) = bytes else {
            self.outcome = SessionRoadsOutcome::Absent { path };
            return Ok(());
        };
        let source_bytes = bytes.len();
        match parse_road_network(&bytes) {
            Ok(network) => {
                let nodes = network.nodes.len();
                let links = network.links.len();
                self.network = Some(network);
                self.outcome = SessionRoadsOutcome::Loaded {
                    path,
                    source_bytes,
                    nodes,
                    links,
                };
                Ok(())
            }
            Err(error) => {
                let detail = format!("{error:?}");
                self.outcome = SessionRoadsOutcome::Failed {
                    path: path.clone(),
                    detail: detail.clone(),
                };
                Err(SessionRoadsCause::Parse(detail))
            }
        }
    }

    /// Explicit teardown: release the network. Always succeeds; there is no
    /// interrupted prefix because parsing is all-or-nothing.
    pub fn destroy(&mut self) -> Result<(), SessionRoadsCause> {
        self.network = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "lhr-session-roads-{}-{}",
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

    fn fixture(nodes: usize, links: usize) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&crate::road_network::ROAD_VERSION.to_le_bytes());
        bytes.extend_from_slice(&(nodes as u32).to_le_bytes());
        bytes.extend(vec![0u8; nodes * crate::road_network::ROAD_NODE_SIZE]);
        bytes.extend_from_slice(&(links as u32).to_le_bytes());
        bytes.extend(vec![0u8; links * crate::road_network::ROAD_LINK_SIZE]);
        bytes
    }

    #[test]
    fn absent_file_retains_nothing_and_succeeds() {
        let scratch = Scratch::new();
        let mut roads = SessionRoads::new();
        roads.load(&scratch.installation(), "road.bin").unwrap();
        assert_eq!(
            roads.outcome(),
            &SessionRoadsOutcome::Absent {
                path: "road.bin".into()
            }
        );
        assert_eq!(roads.node_count(), 0);
        assert_eq!(roads.link_count(), 0);
        assert!(roads.network().is_none());
        roads.destroy().unwrap();
    }

    #[test]
    fn loaded_network_reports_counts_and_releases_on_destroy() {
        let scratch = Scratch::new();
        scratch.write("road.bin", &fixture(3, 7));
        let mut roads = SessionRoads::new();
        roads.load(&scratch.installation(), "road.bin").unwrap();
        match roads.outcome() {
            SessionRoadsOutcome::Loaded { nodes, links, .. } => {
                assert_eq!(*nodes, 3);
                assert_eq!(*links, 7);
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
        assert_eq!(roads.node_count(), 3);
        assert_eq!(roads.link_count(), 7);
        assert!(roads.network().is_some());
        roads.destroy().unwrap();
        assert_eq!(roads.node_count(), 0);
        assert!(roads.network().is_none());
        roads.destroy().unwrap();
    }

    #[test]
    fn second_load_is_rejected_without_touching_state() {
        let scratch = Scratch::new();
        let mut roads = SessionRoads::new();
        roads.load(&scratch.installation(), "road.bin").unwrap();
        let error = roads.load(&scratch.installation(), "road.bin").unwrap_err();
        assert!(matches!(error, SessionRoadsCause::AlreadyAttempted));
        roads.destroy().unwrap();
    }

    #[test]
    fn corrupt_payload_fails_and_retains_nothing() {
        let scratch = Scratch::new();
        scratch.write("road.bin", &[1, 2, 3]);
        let mut roads = SessionRoads::new();
        let error = roads.load(&scratch.installation(), "road.bin").unwrap_err();
        assert!(matches!(error, SessionRoadsCause::Parse(_)));
        assert!(matches!(
            roads.outcome(),
            SessionRoadsOutcome::Failed { .. }
        ));
        assert!(roads.network().is_none());
        roads.destroy().unwrap();
    }
}
