//! Bounded `road.bin` intake for the session road network.
//!
//! Reference: REA Ghidra analysis of Game.exe
//! (SHA-256 `08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27`),
//! `C_roads::LoadData` at PC 0x559e60 (mafia-re `d0fbd6d` file 70,
//! `road_web.cpp`, where it is still a stub). REA evidence
//! `ev_4ac5d63c9e8e25e8acc380364b2c6d1d4b8941e25f22bfeb27980cd212e9a51c`
//! (324 instructions, 55 basic blocks) establishes the byte format; member
//! meanings beyond the two normalized float slots are not interpreted.
//!
//! Format (all little-endian):
//! - u32 version, must be 2 or the loader reads nothing (`return 0`).
//! - u32 node count, then nodes of 0xEC (236) bytes each, read in one bulk
//!   `dtaRead` guarded by the chunk's remaining length.
//! - u32 link count, then links of 0x18 (24) bytes each, same guard.
//! - Post pass over nodes: float at +0x10 snaps to 13.3 (`0x4154CCCD`) when
//!   `|value - 16.666| < 1.0`, else it is kept.
//! - Post pass over links: same rule for the float at +0x0C.
//!
//! Native follow-ups after the reads (`SortNodesByX`/`RecompileDistances`
//! 0x55c890/0x55a620 and the virtual call) are separate dependencies and are
//! not claimed here. `MergeData` 0x55a210 and `FindPath` 0x55c4e0 remain open.
#![forbid(unsafe_code)]

/// Version word admitted by `LoadData`; anything else reads nothing.
pub const ROAD_VERSION: u32 = 2;
/// Node record size in bytes (`count1 * 0xEC` bulk read).
pub const ROAD_NODE_SIZE: usize = 0xEC;
/// Link record size in bytes (`count2 * 0x18` bulk read).
pub const ROAD_LINK_SIZE: usize = 0x18;
/// Snap comparison base for the two normalized float slots.
pub const ROAD_SNAP_BASE: f32 = 16.666;
/// Snap replacement (`0x4154CCCD`, observed as `13.3` in shipped files).
pub const ROAD_SNAP_VALUE: u32 = 0x4154_CCCD;
/// Node float slot normalized by the post pass.
pub const ROAD_NODE_SNAP_OFFSET: usize = 0x10;
/// Link float slot normalized by the post pass.
pub const ROAD_LINK_SNAP_OFFSET: usize = 0x0C;

/// Host quotas; shipped `mise01/road.bin` (229 nodes, 1124 links) is far below.
const MAX_NODES: usize = 1_000_000;
const MAX_LINKS: usize = 8_000_000;
const MAX_BYTES: usize = 64 * 1024 * 1024;

/// Opaque node record; only the +0x10 float slot has an evidenced reader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoadNode {
    pub bytes: [u8; ROAD_NODE_SIZE],
}

/// Opaque link record; only the +0x0C float slot has an evidenced reader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoadLink {
    pub bytes: [u8; ROAD_LINK_SIZE],
}

/// Owned road network: counted node/link arrays after the snap pass.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoadNetwork {
    pub nodes: Vec<RoadNode>,
    pub links: Vec<RoadLink>,
}

/// Parse failure; the input bytes stay caller-owned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoadParseError {
    Truncated { expected: usize, observed: usize },
    VersionMismatch { expected: u32, observed: u32 },
    QuotaExceeded { what: &'static str },
}

fn word_at(bytes: &[u8], offset: usize) -> Result<u32, RoadParseError> {
    bytes
        .get(offset..offset + 4)
        .ok_or(RoadParseError::Truncated {
            expected: offset + 4,
            observed: bytes.len(),
        })
        .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
}

/// Apply the native snap rule to one float slot in place.
fn snap_slot(record: &mut [u8], offset: usize) {
    let value = f32::from_le_bytes(record[offset..offset + 4].try_into().unwrap());
    if (value - ROAD_SNAP_BASE).abs() < 1.0 {
        record[offset..offset + 4].copy_from_slice(&ROAD_SNAP_VALUE.to_le_bytes());
    }
}

/// Parse `road.bin` through the evidenced layout. Version mismatch reads
/// nothing and reports the observed version (native `return 0`).
pub fn parse_road_network(bytes: &[u8]) -> Result<RoadNetwork, RoadParseError> {
    if bytes.len() > MAX_BYTES {
        return Err(RoadParseError::QuotaExceeded { what: "file" });
    }
    let version = word_at(bytes, 0)?;
    if version != ROAD_VERSION {
        return Err(RoadParseError::VersionMismatch {
            expected: ROAD_VERSION,
            observed: version,
        });
    }
    let node_count = word_at(bytes, 4)? as usize;
    if node_count > MAX_NODES {
        return Err(RoadParseError::QuotaExceeded { what: "nodes" });
    }
    let nodes_len = node_count
        .checked_mul(ROAD_NODE_SIZE)
        .ok_or(RoadParseError::QuotaExceeded { what: "nodes" })?;
    let links_at = 8usize
        .checked_add(nodes_len)
        .ok_or(RoadParseError::QuotaExceeded { what: "nodes" })?;
    let link_count = word_at(bytes, links_at)? as usize;
    if link_count > MAX_LINKS {
        return Err(RoadParseError::QuotaExceeded { what: "links" });
    }
    let links_len = link_count
        .checked_mul(ROAD_LINK_SIZE)
        .ok_or(RoadParseError::QuotaExceeded { what: "links" })?;
    let end = links_at
        .checked_add(4)
        .and_then(|start| start.checked_add(links_len))
        .ok_or(RoadParseError::QuotaExceeded { what: "links" })?;
    if bytes.len() < end {
        return Err(RoadParseError::Truncated {
            expected: end,
            observed: bytes.len(),
        });
    }
    let mut nodes = Vec::with_capacity(node_count);
    for index in 0..node_count {
        let start = 8 + index * ROAD_NODE_SIZE;
        let mut record = [0u8; ROAD_NODE_SIZE];
        record.copy_from_slice(&bytes[start..start + ROAD_NODE_SIZE]);
        snap_slot(&mut record, ROAD_NODE_SNAP_OFFSET);
        nodes.push(RoadNode { bytes: record });
    }
    let mut links = Vec::with_capacity(link_count);
    for index in 0..link_count {
        let start = links_at + 4 + index * ROAD_LINK_SIZE;
        let mut record = [0u8; ROAD_LINK_SIZE];
        record.copy_from_slice(&bytes[start..start + ROAD_LINK_SIZE]);
        snap_slot(&mut record, ROAD_LINK_SNAP_OFFSET);
        links.push(RoadLink { bytes: record });
    }
    Ok(RoadNetwork { nodes, links })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(nodes: usize, links: usize, node_fill: u8, link_fill: u8) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&ROAD_VERSION.to_le_bytes());
        bytes.extend_from_slice(&(nodes as u32).to_le_bytes());
        bytes.extend(vec![node_fill; nodes * ROAD_NODE_SIZE]);
        bytes.extend_from_slice(&(links as u32).to_le_bytes());
        bytes.extend(vec![link_fill; links * ROAD_LINK_SIZE]);
        bytes
    }

    #[test]
    fn empty_network_round_trips() {
        let network = parse_road_network(&fixture(0, 0, 0, 0)).unwrap();
        assert_eq!(network.nodes.len(), 0);
        assert_eq!(network.links.len(), 0);
    }

    #[test]
    fn version_mismatch_reads_nothing() {
        let mut bytes = fixture(1, 1, 0xaa, 0xbb);
        bytes[0] = 3;
        assert_eq!(
            parse_road_network(&bytes),
            Err(RoadParseError::VersionMismatch {
                expected: 2,
                observed: 3
            })
        );
        assert!(parse_road_network(&[]).is_err());
        assert!(parse_road_network(&[2, 0, 0]).is_err());
    }

    #[test]
    fn snap_rule_pins_near_base_and_keeps_distant_values() {
        // Node 0 slot = 16.666 (snaps), node 1 slot = 100.0 (kept).
        let mut bytes = fixture(2, 1, 0, 0);
        bytes[8 + ROAD_NODE_SNAP_OFFSET..8 + ROAD_NODE_SNAP_OFFSET + 4]
            .copy_from_slice(&16.666f32.to_le_bytes());
        bytes[8 + ROAD_NODE_SIZE + ROAD_NODE_SNAP_OFFSET
            ..8 + ROAD_NODE_SIZE + ROAD_NODE_SNAP_OFFSET + 4]
            .copy_from_slice(&100.0f32.to_le_bytes());
        // Link slot = 16.0 (within 1.0 of base, snaps).
        let link_at = 8 + 2 * ROAD_NODE_SIZE + 4;
        bytes[link_at + ROAD_LINK_SNAP_OFFSET..link_at + ROAD_LINK_SNAP_OFFSET + 4]
            .copy_from_slice(&16.0f32.to_le_bytes());
        let network = parse_road_network(&bytes).unwrap();
        assert_eq!(
            u32::from_le_bytes(
                network.nodes[0].bytes[ROAD_NODE_SNAP_OFFSET..ROAD_NODE_SNAP_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            ROAD_SNAP_VALUE
        );
        assert_eq!(
            f32::from_le_bytes(
                network.nodes[1].bytes[ROAD_NODE_SNAP_OFFSET..ROAD_NODE_SNAP_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            100.0
        );
        assert_eq!(
            u32::from_le_bytes(
                network.links[0].bytes[ROAD_LINK_SNAP_OFFSET..ROAD_LINK_SNAP_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            ROAD_SNAP_VALUE
        );
    }

    #[test]
    fn truncation_reports_expected_extent() {
        let bytes = fixture(2, 3, 0xaa, 0xbb);
        let truncated = &bytes[..bytes.len() - 1];
        assert_eq!(
            parse_road_network(truncated),
            Err(RoadParseError::Truncated {
                expected: bytes.len(),
                observed: bytes.len() - 1
            })
        );
        // Missing link-count word.
        let cut = &bytes[..8 + 2 * ROAD_NODE_SIZE];
        assert!(matches!(
            parse_road_network(cut),
            Err(RoadParseError::Truncated { .. })
        ));
    }

    #[test]
    fn quotas_reject_absurd_counts_before_reading() {
        let mut bytes = fixture(0, 0, 0, 0);
        bytes[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(
            parse_road_network(&bytes),
            Err(RoadParseError::QuotaExceeded { what: "nodes" })
        );
    }
}
