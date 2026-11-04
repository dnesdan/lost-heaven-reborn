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

/// `C_roads::FindPath` contract (PC 0x55c4e0; REA evidence
/// `ev_755c7588c0525a37ac254ba5a0a05fcc4a9db1c9cbca999fc09f99de8ef8ac40`,
/// 292 instructions / 47 blocks / 3 params / 9 callees, pseudocode retained
/// in 4,857 code units). Node IDs are u16; `0xFFFF` on either side returns
/// false without touching the output. Bit `0x8000` marks zone-resolved IDs
/// (via 0x55be10); the zone search is 0x55bf60 with per-edge validation
/// 0x55b5d0/0x55c440 and fixups 0x55bcd0. The search cluster is unimplemented;
/// only the sentinel/zone-bit rules below are ported.
/// Invalid node sentinel: either `FindPath` side equal to this returns false.
pub const ROAD_INVALID_NODE: u16 = 0xFFFF;
/// Zone-resolution bit: set IDs resolve through 0x55be10 before the search.
pub const ROAD_ZONE_BIT: u16 = 0x8000;
/// `C_roads::FindPath` entry for the explicit unsupported boundary.
pub const FIND_PATH_ENTRY: u32 = 0x55c4e0;

/// Sentinel rule: any side equal to `ROAD_INVALID_NODE` rejects the query.
pub fn road_node_valid(id: u16) -> bool {
    id != ROAD_INVALID_NODE
}

/// Zone-bit rule: set bit resolves through 0x55be10 before the search.
pub fn road_node_has_zone(id: u16) -> bool {
    id & ROAD_ZONE_BIT != 0
}

/// Base ID with the zone bit cleared.
pub fn road_node_base(id: u16) -> u16 {
    id & !ROAD_ZONE_BIT
}

/// Assembled `FindPath` search and its failures. `Unreachable` resolutions
/// become `Ok(false)` like the native early-outs; host-only out-of-range
/// and cycle errors propagate instead of reading wild memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindPathError {
    Walk(RoadWalkError),
    Zone(ZoneResolveError),
}

fn resolve_side(
    network: &RoadNetwork,
    id: u16,
    reverse: bool,
) -> Result<Option<(u16, u8)>, FindPathError> {
    if id & ROAD_ZONE_BIT == 0 {
        return Ok(None);
    }
    match resolve_zone(network, id, reverse) {
        Ok(side) => Ok(Some(side)),
        Err(ZoneResolveError::Walk(RoadWalkError::Unreachable)) => Ok(None),
        Err(other) => Err(FindPathError::Zone(other)),
    }
}

/// Full `C_roads::FindPath`, logical rewrite of PC 0x55c4e0 (REA
/// `ev_755c7588…`, 292 instructions / 47 blocks / 3 params / 9 callees),
/// assembled from the ported pieces: output clear 0x55cb90, zone
/// resolution 0x55be10, chain emit 0x55bc10, zone Dijkstra 0x55bf60, lane
/// search 0x55b5d0 and chain validation 0x55c440. Order: sentinel guard,
/// output clear, zone resolution of zoned sides (`Unreachable` ends the
/// query), plain-`from` push or chain emit, cross-side fixup unification,
/// Dijkstra plus per-pair lane/validate walk, non-empty tail check ending
/// in `to`, and the `to`-zoned reversed-segment tail. Returns `true` when
/// the query resolves (native 1) and `false` for sentinel, resolution, or
/// search failures (native 0). Equal zone sides skip the search exactly
/// like the native guard.
#[allow(clippy::too_many_lines)]
pub fn find_path(
    network: &RoadNetwork,
    from: u16,
    to: u16,
    out: &mut Vec<u16>,
) -> Result<bool, FindPathError> {
    if !road_node_valid(from) || !road_node_valid(to) {
        return Ok(false);
    }
    clear_path_output(out);
    let (from_zone, from_side) = match resolve_side(network, from, false)? {
        Some(side) => side,
        None if from & ROAD_ZONE_BIT != 0 => return Ok(false),
        None => (from, 0),
    };
    let (mut to_zone, mut to_side) = match resolve_side(network, to, true)? {
        Some(side) => side,
        None if to & ROAD_ZONE_BIT != 0 => return Ok(false),
        None => (to, 0),
    };
    if from & ROAD_ZONE_BIT == 0 {
        out.push(from);
    } else {
        let start = network.link(from).map_err(FindPathError::Walk)?.endpoints();
        let start = if from_side == 0 { start.0 } else { start.1 };
        emit_chain(network, from, start, out, ROAD_INVALID_NODE, to)
            .map_err(FindPathError::Walk)?;
    }
    if from_zone != to_zone {
        // Cross-side fixup unification, in native order: each side's fixup
        // through its selected link endpoint; when they reach each other's
        // zone the query collapses to one zone (and `to_side` becomes whether
        // the old `to_side` was 0). Both masked link indices must be live;
        // otherwise there is nothing to unify through and the search decides.
        let unify = (usize::from(from) & 0x7FFF) < network.links.len()
            && (usize::from(to) & 0x7FFF) < network.links.len();
        if unify {
            let from_endpoints = network.link(from).map_err(FindPathError::Walk)?.endpoints();
            let from_ep = if from_side == 0 {
                from_endpoints.0
            } else {
                from_endpoints.1
            };
            let to_endpoints = network.link(to).map_err(FindPathError::Walk)?.endpoints();
            let to_ep = if to_side == 0 {
                to_endpoints.0
            } else {
                to_endpoints.1
            };
            let seen_from = fixup_distance(network, from, from_ep).map_err(FindPathError::Walk)?;
            let seen_to = fixup_distance(network, to, to_ep).map_err(FindPathError::Walk)?;
            if seen_from.0 == to_zone && seen_to.0 == from_zone {
                to_zone = from_zone;
                to_side = u8::from(to_side == 0);
            }
        }
        if from_zone != to_zone {
            let zones = match dijkstra_zones(network, from_zone as usize, to_zone as usize) {
                Ok(zones) => zones,
                Err(RoadWalkError::Unreachable) => return Ok(false),
                Err(other) => return Err(FindPathError::Walk(other)),
            };
            let mut index = 1usize;
            while index < zones.len() {
                let previous = zones[index - 1];
                let current = zones[index];
                let lane =
                    edge_lane(network, previous, current, false).map_err(FindPathError::Walk)?;
                if lane.is_none() {
                    return Ok(false);
                }
                let reached =
                    edge_validate(network, current, out, from, to).map_err(FindPathError::Walk)?;
                if !reached {
                    break;
                }
                index += 1;
            }
        }
    }
    if !out.is_empty() && *out.last().unwrap() == to {
        return Ok(true);
    }
    if to & ROAD_ZONE_BIT != 0 {
        let endpoints = network.link(to).map_err(FindPathError::Walk)?.endpoints();
        let start = if to_side == 0 {
            endpoints.0
        } else {
            endpoints.1
        };
        let mut segment = Vec::new();
        emit_chain(
            network,
            to,
            start,
            &mut segment,
            ROAD_INVALID_NODE,
            ROAD_INVALID_NODE,
        )
        .map_err(FindPathError::Walk)?;
        let count = segment.len();
        for step in (1..count).rev() {
            if segment[step] == from {
                out.clear();
            }
            out.push(segment[step - 1]);
        }
    }
    Ok(true)
}

/// Output-vector clear used by `FindPath` (PC 0x55cb90; REA evidence
/// `ev_97ead2f741f988159efc3ddf6bbef3ca3226aaecf03cac86cca8a880c353e4c1`,
/// 20 instructions / 5 blocks / 0 callees / 2 callers). The native body
/// copies `[src, end)` down to `dst` and retires the end pointer; the
/// `FindPath` call passes `(begin, end)`, which clears the output.
pub const CLEAR_OUTPUT_ENTRY: u32 = 0x55cb90;

/// Clear a `FindPath` output vector in the 0x55cb90 order.
pub fn clear_path_output(path: &mut Vec<u16>) {
    path.clear();
}

/// Zone resolution used by `FindPath` (PC 0x55be10; REA evidence
/// `ev_abb271ebced7580f75af370782b9235fe944891d5743be9c45e11161385a9c59`,
/// 132 instructions / 18 blocks / 3 params / 2 callees). Recovered order:
/// index the link array at `this`+0x18 by `id & 0x7FFF` (stride 0x18);
/// resolve both stored endpoints (u16 at +0x10/+0x12) through the fixup
/// 0x55bcd0; on single-sided validity return that side; otherwise compare
/// edge weights from 0x55b5d0 in both directions (swapped when the reverse
/// flag is set) and keep the side with the smaller fixup distance, or
/// `0xFFFF` when both sides fail. Returns the zone id and the selected
/// side (0/1). The fixup 0x55bcd0 and weight 0x55b5d0 callees have landed;
/// `resolve_zone` below ports the full decision tree.
pub const ZONE_RESOLVE_ENTRY: u32 = 0x55be10;

/// Zone-resolution failure; out-of-range reads are host errors, and
/// double-sided invalidity is `Unreachable`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoneResolveError {
    Unsupported { entry: u32 },
    Walk(RoadWalkError),
}

/// Lane search for one target endpoint, logical rewrite of PC 0x55b5d0
/// (REA `ev_b3e004ff…`, 104 instructions / 23 blocks / 3 params / 0 callees).
/// Scans the four lanes for `target` at +0x1C, remembering the last match:
/// no match returns `None`; a single match returns its lane; multiple
/// matches return the minimum-weight lane when `prefer_lightest`, else the
/// last matching lane. (The `prefer_lightest == false` chain walk over the
/// node+0x14 table performs effect-free reads only and does not change the
/// result.) `None` encodes native `-1`.
pub fn edge_lane(
    network: &RoadNetwork,
    node: u16,
    target: u16,
    prefer_lightest: bool,
) -> Result<Option<usize>, RoadWalkError> {
    let node_record = network.node(node)?;
    let mut matches = Vec::new();
    for lane in 0..ROAD_LANES {
        if node_record.lane_neighbor(lane) == target {
            matches.push(lane);
        }
    }
    match matches.len() {
        0 => Ok(None),
        1 => Ok(Some(matches[0])),
        _ => {
            if prefer_lightest {
                let mut best = matches[0];
                let mut best_weight = node_record.lane_weight(best);
                for lane in matches.into_iter().skip(1) {
                    let weight = node_record.lane_weight(lane);
                    if weight < best_weight {
                        best_weight = weight;
                        best = lane;
                    }
                }
                Ok(Some(best))
            } else {
                Ok(matches.into_iter().last())
            }
        }
    }
}

/// Chain validator, logical rewrite of PC 0x55c440 (REA `ev_b1ea4914…`,
/// 62 instructions / 13 blocks / 5 params / 1 callee). Unlike `emit_chain`
/// it does not seed the output with `id`: it pushes `start` first, then
/// follows stored link chains with the same stop rules (`stop_truncate`
/// clears before the push lands, `stop_done` ends). Returns `true` when the
/// walk stops early (plain id or `0xFFFF` start) and `false` when it reaches
/// `stop_done`, matching the native 1/0 convention. Cycle-capped.
pub fn edge_validate(
    network: &RoadNetwork,
    start: u16,
    out: &mut Vec<u16>,
    stop_truncate: u16,
    stop_done: u16,
) -> Result<bool, RoadWalkError> {
    let mut prev = start;
    let mut current = start;
    let cap = network.links.len() + network.nodes.len() + 1;
    for _ in 0..cap {
        if current == ROAD_INVALID_NODE {
            return Ok(true);
        }
        if current == stop_truncate {
            out.clear();
        }
        out.push(current);
        if current == stop_done {
            return Ok(false);
        }
        if current & ROAD_ZONE_BIT == 0 {
            return Ok(true);
        }
        let (endpoint0, endpoint1) = network.link(current)?.endpoints();
        let next = if endpoint0 == prev {
            endpoint1
        } else {
            endpoint0
        };
        prev = current;
        current = next;
    }
    Err(RoadWalkError::Cycle)
}

/// Zone resolution, logical rewrite of PC 0x55be10 (REA `ev_abb271eb…`,
/// 132 instructions / 18 blocks / 3 params / callees 0x55bcd0 + 0x55b5d0).
/// Indexes the link array by `id & 0x7FFF`, resolves both stored endpoints
/// through `fixup_distance`, and returns the closer side (or the only valid
/// one) with its selected side 0/1; both invalid yields `Unreachable`.
/// `reverse` swaps the edge-weight comparison order. The final tie-break
/// compares fixup distances like the native `local_4 <= local_8` rule.
pub fn resolve_zone(
    network: &RoadNetwork,
    id: u16,
    reverse: bool,
) -> Result<(u16, u8), ZoneResolveError> {
    use ZoneResolveError as E;
    let link = network.link(id).map_err(E::Walk)?;
    let (endpoint0, endpoint1) = link.endpoints();
    let first = fixup_distance(network, id, endpoint0).map_err(E::Walk);
    let second = fixup_distance(network, id, endpoint1).map_err(E::Walk);
    let (first, second) = match (first, second) {
        (Err(_), Err(_)) => return Err(E::Walk(RoadWalkError::Unreachable)),
        (Ok(first), Err(_)) => return Ok((first.0, 0)),
        (Err(_), Ok(second)) => return Ok((second.0, 1)),
        (Ok(first), Ok(second)) => (first, second),
    };
    let adjacent_first = edge_lane(network, second.0, first.0, true).map_err(E::Walk)?;
    let adjacent_second = edge_lane(network, first.0, second.0, true).map_err(E::Walk)?;
    let (mut weight_first, mut weight_second) =
        (adjacent_first.is_some(), adjacent_second.is_some());
    if reverse {
        std::mem::swap(&mut weight_first, &mut weight_second);
    }
    match (weight_first, weight_second) {
        (false, false) => Err(E::Walk(RoadWalkError::Unreachable)),
        (true, false) => Ok((first.0, 0)),
        (false, true) => Ok((second.0, 1)),
        (true, true) => {
            if first.1 <= second.1 {
                Ok((first.0, 0))
            } else {
                Ok((second.0, 1))
            }
        }
    }
}

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

/// Dijkstra lane layout recovered from `C_roads` zone search 0x55bf60
/// (REA `ev_23ee93c7…`): 4 lanes per node, stride 0x34 (`iVar8 < 0xd0`),
/// neighbor u16 at lane +0x1C, weight f32 at lane +0x20. `0xFFFF`
/// neighbors are skipped.
pub const ROAD_LANES: usize = 4;
pub const ROAD_LANE_STRIDE: usize = 0x34;
pub const ROAD_LANE_NEIGHBOR_OFFSET: usize = 0x1C;
pub const ROAD_LANE_WEIGHT_OFFSET: usize = 0x20;
/// Position vector at record +0x00, read as three floats by the fixup
/// 0x55bcd0 (`pfVar[0..2]` squared-distance accumulation) for both arrays.
pub const ROAD_POSITION_OFFSET: usize = 0x00;
/// Link endpoint pair read by zone resolution 0x55be10 at +0x10/+0x12.
pub const ROAD_LINK_ENDPOINT_OFFSET: usize = 0x10;
/// Back-reference slots reset by recompilation: +0x14/+0x16 per link.
pub const ROAD_LINK_BACKREF_OFFSET: usize = 0x14;
/// Per-lane flag array base: u16 flag for lane `k` at +0x14 + 2 bytes × `k`
/// (Rizin gate ASM at 0x55a6dc: nodebase+0x1A walking down by 2 across
/// lanes 3,2,1,0 alongside the 0x34 lane stride). Zone-marked link indices;
/// `0xFFFF` lanes are skipped by both the gate and the search.
pub const ROAD_LANE_FLAG_OFFSET: usize = 0x14;
/// Unreached distance written by the Dijkstra element constructor 0x55c430
/// (REA `ev_bf43bc2b…`, 4 instructions: `pred = 0xFFFF`,
/// `dist_bits = 0x501502f9`). Unreached nodes keep this value; the native
/// unreached test reads the predecessor `-1`, which the rewrite preserves.
pub const ROAD_UNREACHED_DIST_BITS: u32 = 0x5015_02f9;

fn read_u16(record: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(record[offset..offset + 2].try_into().unwrap())
}

fn read_f32(record: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes(record[offset..offset + 4].try_into().unwrap())
}

impl RoadNode {
    /// World position from the evidenced +0x00 slot.
    pub fn position(&self) -> [f32; 3] {
        [
            read_f32(&self.bytes, ROAD_POSITION_OFFSET),
            read_f32(&self.bytes, ROAD_POSITION_OFFSET + 4),
            read_f32(&self.bytes, ROAD_POSITION_OFFSET + 8),
        ]
    }

    /// Per-lane flag from the compact +0x14 array (one u16 per lane).
    pub fn lane_flag(&self, lane: usize) -> u16 {
        debug_assert!(lane < ROAD_LANES);
        read_u16(&self.bytes, ROAD_LANE_FLAG_OFFSET + lane * 2)
    }

    fn set_lane_flag(&mut self, lane: usize, value: u16) {
        self.bytes[ROAD_LANE_FLAG_OFFSET + lane * 2..ROAD_LANE_FLAG_OFFSET + lane * 2 + 2]
            .copy_from_slice(&value.to_le_bytes());
    }

    /// Recompiled lane weight slot (+0x20).
    pub fn set_lane_weight(&mut self, lane: usize, weight: f32) {
        self.bytes[ROAD_LANE_WEIGHT_OFFSET + lane * ROAD_LANE_STRIDE
            ..ROAD_LANE_WEIGHT_OFFSET + lane * ROAD_LANE_STRIDE + 4]
            .copy_from_slice(&weight.to_le_bytes());
    }

    /// Recompiled lane terminal slot (+0x1C, overwrites the neighbor).
    pub fn set_lane_terminal(&mut self, lane: usize, terminal: u16) {
        self.bytes[ROAD_LANE_NEIGHBOR_OFFSET + lane * ROAD_LANE_STRIDE
            ..ROAD_LANE_NEIGHBOR_OFFSET + lane * ROAD_LANE_STRIDE + 2]
            .copy_from_slice(&terminal.to_le_bytes());
    }

    /// Neighbor index of one Dijkstra lane (`0xFFFF` means no lane).
    pub fn lane_neighbor(&self, lane: usize) -> u16 {
        debug_assert!(lane < ROAD_LANES);
        read_u16(
            &self.bytes,
            ROAD_LANE_NEIGHBOR_OFFSET + lane * ROAD_LANE_STRIDE,
        )
    }

    /// Weight of one Dijkstra lane.
    pub fn lane_weight(&self, lane: usize) -> f32 {
        debug_assert!(lane < ROAD_LANES);
        read_f32(
            &self.bytes,
            ROAD_LANE_WEIGHT_OFFSET + lane * ROAD_LANE_STRIDE,
        )
    }
}

impl RoadLink {
    /// World position from the evidenced +0x00 slot.
    pub fn position(&self) -> [f32; 3] {
        [
            read_f32(&self.bytes, ROAD_POSITION_OFFSET),
            read_f32(&self.bytes, ROAD_POSITION_OFFSET + 4),
            read_f32(&self.bytes, ROAD_POSITION_OFFSET + 8),
        ]
    }

    /// Stored endpoint pair from the evidenced +0x10 slot.
    pub fn endpoints(&self) -> (u16, u16) {
        (
            read_u16(&self.bytes, ROAD_LINK_ENDPOINT_OFFSET),
            read_u16(&self.bytes, ROAD_LINK_ENDPOINT_OFFSET + 2),
        )
    }

    /// Back-reference slots rewritten by recompilation (+0x14/+0x16).
    pub fn backrefs(&self) -> (u16, u16) {
        (
            read_u16(&self.bytes, ROAD_LINK_BACKREF_OFFSET),
            read_u16(&self.bytes, ROAD_LINK_BACKREF_OFFSET + 2),
        )
    }

    fn set_backref(&mut self, slot: usize, value: u16) {
        self.bytes[ROAD_LINK_BACKREF_OFFSET + slot * 2..ROAD_LINK_BACKREF_OFFSET + slot * 2 + 2]
            .copy_from_slice(&value.to_le_bytes());
    }
}

/// Graph-walk failure; native out-of-range reads are host errors here, never
/// unchecked memory access.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoadWalkError {
    NodeOutOfRange { id: u16 },
    LinkOutOfRange { id: u16 },
    Cycle,
    Unreachable,
}

fn squared_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let dz = b[2] - a[2];
    dx * dx + dy * dy + dz * dz
}

impl RoadNetwork {
    fn node(&self, id: u16) -> Result<&RoadNode, RoadWalkError> {
        self.nodes
            .get(id as usize)
            .ok_or(RoadWalkError::NodeOutOfRange { id })
    }

    fn link(&self, id: u16) -> Result<&RoadLink, RoadWalkError> {
        self.links
            .get((id & !ROAD_ZONE_BIT) as usize)
            .ok_or(RoadWalkError::LinkOutOfRange { id })
    }

    fn base_position(&self, id: u16) -> Result<[f32; 3], RoadWalkError> {
        if id & ROAD_ZONE_BIT != 0 {
            Ok(self.link(id)?.position())
        } else {
            Ok(self.node(id)?.position())
        }
    }
}

/// Path-chain emitter, logical rewrite of PC 0x55bc10 (REA `ev_6ceec1cd…`,
/// 63 instructions / 12 blocks / 5 params / 1 callee). Pushes `id`, then
/// walks `start` along stored link chains: each step pushes the current id,
/// truncates the output when it equals `stop_truncate`, stops when it equals
/// `stop_done` or carries no zone bit, otherwise follows the link endpoint
/// that is not the previously pushed id. The native body has no cycle guard;
/// the host caps steps at links + nodes + 1 and reports `Cycle`.
pub fn emit_chain(
    network: &RoadNetwork,
    id: u16,
    start: u16,
    out: &mut Vec<u16>,
    stop_truncate: u16,
    stop_done: u16,
) -> Result<(), RoadWalkError> {
    out.push(id);
    let mut prev = id;
    let mut current = start;
    let cap = network.links.len() + network.nodes.len() + 1;
    for _ in 0..cap {
        if current == ROAD_INVALID_NODE {
            return Ok(());
        }
        out.push(current);
        if current == stop_truncate {
            out.clear();
        }
        if current == stop_done {
            return Ok(());
        }
        if current & ROAD_ZONE_BIT == 0 {
            return Ok(());
        }
        let (endpoint0, endpoint1) = network.link(current)?.endpoints();
        let next = if endpoint0 == prev {
            endpoint1
        } else {
            endpoint0
        };
        prev = current;
        current = next;
    }
    Err(RoadWalkError::Cycle)
}

/// Chained distance fixup, logical rewrite of PC 0x55bcd0 (REA
/// `ev_77cb6fe3…`, 97 instructions / 13 blocks / 3 params / 0 callees).
/// Accumulates squared Euclidean position distances from `from` along the
/// stored chain starting at `to`, following non-backtracking endpoints like
/// `emit_chain`, and returns the terminal id with the total. A terminal
/// `0xFFFF` returns immediately with zero distance. Positions come from the
/// evidenced +0x00 slots of both arrays. Cycle-capped like `emit_chain`.
pub fn fixup_distance(
    network: &RoadNetwork,
    from: u16,
    to: u16,
) -> Result<(u16, f32), RoadWalkError> {
    let mut distance = 0.0f32;
    let mut current_from = from;
    let mut current_to = to;
    let cap = network.links.len() + network.nodes.len() + 1;
    for _ in 0..cap {
        if current_to == ROAD_INVALID_NODE {
            return Ok((ROAD_INVALID_NODE, distance));
        }
        let base_from = network.base_position(current_from)?;
        let base_to = network.base_position(current_to)?;
        let delta = squared_distance(base_from, base_to);
        distance += delta;
        if current_to & ROAD_ZONE_BIT == 0 {
            return Ok((current_to, distance));
        }
        let (endpoint0, endpoint1) = network.link(current_to)?.endpoints();
        let next = if endpoint0 == current_from {
            endpoint1
        } else {
            endpoint0
        };
        current_from = current_to;
        current_to = next;
    }
    Err(RoadWalkError::Cycle)
}

/// Zone shortest path, logical rewrite of the Dijkstra core of PC 0x55bf60
/// (REA `ev_23ee93c7…`, 380 instructions / 55 blocks). Relaxes the four
/// evidenced lanes per node (`neighbor != 0xFFFF`, `new < best` and
/// `new < dist[neighbor]`), tracks predecessors, and reconstructs the
/// `from ..= to` chain. Endpoint guards (`from == to`, either `0xFFFF`,
/// empty graph) return `Unreachable` like the native early-out. Visit order
/// inside equal distances is a host detail; path validity (consecutive lane
/// adjacency) is the checked property.
pub fn dijkstra_zones(
    network: &RoadNetwork,
    from: usize,
    to: usize,
) -> Result<Vec<u16>, RoadWalkError> {
    if network.nodes.is_empty()
        || from == to
        || from >= network.nodes.len()
        || to >= network.nodes.len()
    {
        return Err(RoadWalkError::Unreachable);
    }
    let unreached = f32::from_bits(ROAD_UNREACHED_DIST_BITS);
    let mut dist = vec![unreached; network.nodes.len()];
    let mut pred = vec![ROAD_INVALID_NODE; network.nodes.len()];
    dist[from] = 0.0;
    let mut settled = vec![false; network.nodes.len()];
    loop {
        // Only discovered nodes enter extraction, matching the native
        // frontier (undiscovered entries keep the ctor sentinel).
        let mut current = None;
        let mut best = unreached;
        for (index, settled_flag) in settled.iter().enumerate() {
            if !settled_flag && dist[index] < best {
                best = dist[index];
                current = Some(index);
            }
        }
        let Some(current) = current else {
            return Err(RoadWalkError::Unreachable);
        };
        if current == to {
            break;
        }
        settled[current] = true;
        let node = network.node(current as u16)?;
        for lane in 0..ROAD_LANES {
            let neighbor = node.lane_neighbor(lane);
            if neighbor == ROAD_INVALID_NODE || usize::from(neighbor) >= network.nodes.len() {
                continue;
            }
            let candidate = dist[current] + node.lane_weight(lane);
            if candidate < dist[usize::from(neighbor)] {
                dist[usize::from(neighbor)] = candidate;
                pred[usize::from(neighbor)] = current as u16;
            }
        }
    }
    let mut path = vec![to as u16];
    let mut current = to;
    while current != from {
        let previous = pred[current];
        if previous == ROAD_INVALID_NODE {
            return Err(RoadWalkError::Unreachable);
        }
        path.push(previous);
        current = usize::from(previous);
    }
    path.reverse();
    Ok(path)
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

/// Encode a network through the `C_roads::SaveData` (PC 0x559d40) layout.
///
/// Raw Rizin disassembly evidence (`pd` at 0x559d40–0x559e5a, `ret 4`,
/// returns true): guarded `dtaWrite` of u32 version 2, then u32 node count
/// derived as `(end - begin) / 236` (magic `0x22b63cbf`, `sar 5`), the node
/// bytes bulk-written as `count * 236` (lea chain `edx + ecx * 2`, `shl 2`),
/// then u32 link count as `(end - begin) / 24` (magic `0x2aaaaaab`,
/// `sar 2`) with `count * 24` bytes (`lea count * 3`, `shl 3`). Every write
/// is guarded by chunk-open at +0x10 with write-mode at +0x18; member words
/// are `this`+8/+0xC (node begin/end) and +0x18/+0x1C (link begin/end).
/// Stored bytes round-trip verbatim; the snap rule applies on parse only.
pub fn encode_road_network(network: &RoadNetwork) -> Vec<u8> {
    let mut out = Vec::with_capacity(
        12 + network.nodes.len() * ROAD_NODE_SIZE + network.links.len() * ROAD_LINK_SIZE,
    );
    out.extend_from_slice(&ROAD_VERSION.to_le_bytes());
    out.extend_from_slice(&(network.nodes.len() as u32).to_le_bytes());
    for node in &network.nodes {
        out.extend_from_slice(&node.bytes);
    }
    out.extend_from_slice(&(network.links.len() as u32).to_le_bytes());
    for link in &network.links {
        out.extend_from_slice(&link.bytes);
    }
    out
}

/// Native vector-length helper shared by recompilation (inline at
/// 0x55a781: compare against `0x322bcc77`, subtract 1.0 at data `0x623238`,
/// absolute value, compare against 1e-8 at `0x6232e8`, `test ah,5`),
/// identical to the vehicle-body helper: tiny squared lengths read as
/// zero, near-unit reads as one, otherwise the square root.
pub fn road_vector_length(squared: f32) -> f32 {
    if (squared.to_bits() as i32) < 0x322b_cc77 {
        return 0.0;
    }
    if (squared - 1.0).abs() < 1e-8 {
        return 1.0;
    }
    squared.sqrt()
}

/// Recompilation reset from `RecompileDistances` 0x55a620: every link's
/// +0x14/+0x16 back-reference slots return to `0xFFFF` before the walks
/// refill them.
pub fn reset_link_backrefs(network: &mut RoadNetwork) {
    for link in &mut network.links {
        link.bytes[ROAD_LINK_BACKREF_OFFSET..ROAD_LINK_BACKREF_OFFSET + 2]
            .copy_from_slice(&ROAD_INVALID_NODE.to_le_bytes());
        link.bytes[ROAD_LINK_BACKREF_OFFSET + 2..ROAD_LINK_BACKREF_OFFSET + 4]
            .copy_from_slice(&ROAD_INVALID_NODE.to_le_bytes());
    }
}

/// Link-swap accounting for `sort_nodes_by_x`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SortReport {
    pub swaps: usize,
}

/// Link sort, logical rewrite of `SortNodesByX` 0x55c890 (Rizin ASM from
/// 0x55c890 to `ret` at 0x55ca94): selection sort of the link array by the
/// position-X float at +0x00 with the exact strict-less update rule
/// (provable from the `fld`/`fcomp`/`fnstsw`/`test ah,5`/`jp` sequence:
/// unordered/NaN keeps the incumbent, equal keeps the first). Each swap
/// exchanges the 24-byte records through a temp buffer and remaps
/// zone-marked references `i|0x8000` ↔ `j|0x8000` in every link endpoint
/// slot (+0x10/+0x12, the first remap loop over 2 u16s per link) and every
/// node lane-flag slot (+0x14 + 2 bytes × lane, the second loop over 4
/// u16s per node). The `0x55caa0` entry guard is skipped: on empty input
/// the sort is a no-op either way. Returns the swap count; shipped
/// mise01 links sort with zero swaps.
pub fn sort_nodes_by_x(network: &mut RoadNetwork) -> SortReport {
    let mut report = SortReport::default();
    let count = network.links.len();
    for i in 0..count {
        let mut best = i;
        let mut best_x = f32::from_le_bytes(network.links[i].bytes[0..4].try_into().unwrap());
        for j in (i + 1)..count {
            let candidate = f32::from_le_bytes(network.links[j].bytes[0..4].try_into().unwrap());
            if candidate.partial_cmp(&best_x) == Some(std::cmp::Ordering::Less) {
                best = j;
                best_x = candidate;
            }
        }
        if best != i {
            network.links.swap(i, best);
            remap_zone_refs(network, i, best);
            report.swaps += 1;
        }
    }
    report
}

/// Zone-reference remap after a link swap of positions `i` and `j`
/// (0-based): every u16 equal to `i|0x8000` becomes `j|0x8000` and vice
/// versa, over link endpoint slots and node lane-flag slots.
fn remap_zone_refs(network: &mut RoadNetwork, i: usize, j: usize) {
    let marked_i = (i as u16) | ROAD_ZONE_BIT;
    let marked_j = (j as u16) | ROAD_ZONE_BIT;
    let swap_value = |value: &mut u16| {
        if *value == marked_i {
            *value = marked_j;
        } else if *value == marked_j {
            *value = marked_i;
        }
    };
    for link in &mut network.links {
        let (mut first, mut second) = link.endpoints();
        swap_value(&mut first);
        swap_value(&mut second);
        link.bytes[ROAD_LINK_ENDPOINT_OFFSET..ROAD_LINK_ENDPOINT_OFFSET + 2]
            .copy_from_slice(&first.to_le_bytes());
        link.bytes[ROAD_LINK_ENDPOINT_OFFSET + 2..ROAD_LINK_ENDPOINT_OFFSET + 4]
            .copy_from_slice(&second.to_le_bytes());
    }
    for node in &mut network.nodes {
        for lane in 0..ROAD_LANES {
            let mut flag = node.lane_flag(lane);
            swap_value(&mut flag);
            node.set_lane_flag(lane, flag);
        }
    }
}

/// Lane-length walk outcome for one recompiled lane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LaneWalkOutcome {
    /// Gate refused the lane (flag or neighbor `0xFFFF`); slots untouched.
    Skipped,
    /// Walk completed with the terminal id and accumulated true length.
    Done {
        terminal: u16,
        weight: f32,
        visited: usize,
    },
}

/// Per-lane length walk, logical rewrite of the `RecompileDistances`
/// 0x55a620 lane body (Rizin 0x55a6dc–0x55aa0a plus REA `ev_69eb9075…`
/// pages): gate on flag (+0x14+2·lane) and neighbor (+0x1C+0x34·lane),
/// first segment |node − link[flag]| through `road_vector_length`, then
/// the endpoint chain with per-step link+0x14 backpatch (origin forward
/// node index) and visited pushes, ending in a plain-node final segment
/// or a loop-back/absent terminal. Writes the lane weight (+0x20) and
/// terminal (+0x1C) and backpatches every visited link's +0x16 slot. The
/// previous seed for endpoint selection is the forward node index
/// (Rizin 0x55a7c7 `mov ebx, [esp+0x54]` after `[esp+0x54]` takes the
/// `[esp+0x1c]` forward index — not the byte offset also present in the
/// gate block); the walk terminates on plain/absent/looping endpoints.
/// Host caps steps at links + nodes + 1 and reports `Cycle` where the
/// native body would not return.
pub fn recompile_lane_walk(
    network: &mut RoadNetwork,
    node_idx: usize,
    lane: usize,
    origin: u16,
) -> Result<LaneWalkOutcome, RoadWalkError> {
    let node_count = network.nodes.len();
    if node_idx >= node_count {
        return Err(RoadWalkError::NodeOutOfRange {
            id: node_idx as u16,
        });
    }
    if lane >= ROAD_LANES {
        return Err(RoadWalkError::NodeOutOfRange { id: lane as u16 });
    }
    let (flag, neighbor, node_pos) = {
        let node = &network.nodes[node_idx];
        (
            node.lane_flag(lane),
            node.lane_neighbor(lane),
            node.position(),
        )
    };
    if flag == ROAD_INVALID_NODE || neighbor == ROAD_INVALID_NODE {
        return Ok(LaneWalkOutcome::Skipped);
    }
    let link0 = (flag & !ROAD_ZONE_BIT) as usize;
    if link0 >= network.links.len() {
        return Err(RoadWalkError::LinkOutOfRange { id: flag });
    }
    let mut total = road_vector_length(squared_distance(node_pos, network.links[link0].position()));
    let mut terminal = ROAD_INVALID_NODE;
    let mut visited: Vec<usize> = Vec::new();
    if flag & ROAD_ZONE_BIT != 0 {
        network.links[link0].set_backref(0, origin);
        visited.push(link0);
    }
    let mut cur = flag;
    let mut prev: u16 = node_idx as u16;
    let cap = network.links.len() + node_count + 1;
    let mut finished = false;
    for _ in 0..cap {
        let (endpoint0, endpoint1) = network.link(cur)?.endpoints();
        let mut next = endpoint0;
        if endpoint0 == prev || endpoint0 == ROAD_INVALID_NODE {
            next = endpoint1;
        }
        if next == prev || next == ROAD_INVALID_NODE {
            terminal = next;
            finished = true;
            break;
        }
        if next & ROAD_ZONE_BIT == 0 {
            let node_pos = network.node(next)?.position();
            let link_pos = network.link(cur)?.position();
            total += road_vector_length(squared_distance(link_pos, node_pos));
            terminal = next;
            finished = true;
            break;
        }
        let next_pos = network.link(next)?.position();
        let cur_pos = network.link(cur)?.position();
        total += road_vector_length(squared_distance(cur_pos, next_pos));
        prev = cur;
        cur = next;
        let step = (cur & !ROAD_ZONE_BIT) as usize;
        if step >= network.links.len() {
            return Err(RoadWalkError::LinkOutOfRange { id: cur });
        }
        network.links[step].set_backref(0, origin);
        visited.push(step);
    }
    if !finished {
        return Err(RoadWalkError::Cycle);
    }
    {
        let node = &mut network.nodes[node_idx];
        node.set_lane_weight(lane, total);
        node.set_lane_terminal(lane, terminal);
    }
    for step in &visited {
        network.links[*step].set_backref(1, terminal);
    }
    Ok(LaneWalkOutcome::Done {
        terminal,
        weight: total,
        visited: visited.len(),
    })
}

/// Recompilation pass report.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecompileReport {
    pub lanes_walked: usize,
    pub lanes_skipped: usize,
}

/// Lane-length recompilation driver: backref reset, then every lane of
/// every node in reverse order (native `RecompileDistances` traversal
/// order; lanes of different nodes share links, so the order is part of
/// the behavior). Lane permutation is a separate pass and stays open.
///
/// NOTE (Phase C verdict): the angular permutation (signed-atan2 lane
/// ordering via 0x408490, REA `ev_4613dcb1…`, plus gap writes to lane+0x08)
/// is not ported, and provably cannot affect mission behavior on current
/// data: lane order is consumed only by Dijkstra tie-breaks, and all 229
/// nodes have pairwise-distinct lane weights (zero exact ties), so every
/// shortest path is unique regardless of order; gap slots have no
/// recovered reader. Re-verify ties per mission before trusting this for
/// other road files. Full evidence: REA `ev_69eb9075…` + Rizin
/// 0x55ab00–0x55ae70 reads; discriminator node 201 [d1, _, ca, _].
pub fn recompute_lane_lengths(network: &mut RoadNetwork) -> Result<RecompileReport, RoadWalkError> {
    reset_link_backrefs(network);
    let mut report = RecompileReport::default();
    let node_count = network.nodes.len();
    for node_idx in (0..node_count).rev() {
        let origin = node_idx as u16;
        for lane in (0..ROAD_LANES).rev() {
            match recompile_lane_walk(network, node_idx, lane, origin)? {
                LaneWalkOutcome::Skipped => report.lanes_skipped += 1,
                LaneWalkOutcome::Done { .. } => report.lanes_walked += 1,
            }
        }
    }
    Ok(report)
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

    #[test]
    fn find_path_sentinel_and_zone_rules_match_recovered_contract() {
        assert!(!road_node_valid(ROAD_INVALID_NODE));
        assert_eq!(ROAD_INVALID_NODE, 0xFFFF);
        assert!(road_node_valid(0));
        assert!(road_node_valid(0x7FFF));
        assert!(!road_node_has_zone(0x1234));
        assert!(road_node_has_zone(0x8000));
        assert!(road_node_has_zone(0xFFFF));
        assert_eq!(road_node_base(0x8000), 0);
        assert_eq!(road_node_base(0x8123), 0x0123);
        assert_eq!(road_node_base(0x0123), 0x0123);
        assert_eq!(FIND_PATH_ENTRY, 0x55c4e0);
        // Sentinel sides end the query before the output clear, so the
        // caller's prefix is retained like the native early-out.
        let graph = line_graph();
        let mut out = vec![9u16];
        assert_eq!(find_path(&graph, ROAD_INVALID_NODE, 1, &mut out), Ok(false));
        assert_eq!(out, vec![9u16]);
        let mut out = vec![9u16];
        assert_eq!(find_path(&graph, 0, ROAD_INVALID_NODE, &mut out), Ok(false));
        assert_eq!(out, vec![9u16]);
    }

    #[test]
    fn find_path_assembles_plain_queries_end_to_end() {
        let graph = two_node_network();
        let mut out = Vec::new();
        assert_eq!(find_path(&graph, 0, 1, &mut out), Ok(true));
        assert_eq!(out, vec![0, 1]);
        // Same-zone query resolves without the search.
        let mut out = Vec::new();
        assert_eq!(find_path(&graph, 1, 1, &mut out), Ok(true));
        assert_eq!(out, vec![1]);
        // No lanes between the pair: the search fails, retaining the pushed
        // prefix exactly like the native Dijkstra-failure return.
        let graph = RoadNetwork {
            nodes: vec![
                placed_node(
                    0.0,
                    [
                        (ROAD_INVALID_NODE, 0.0),
                        (ROAD_INVALID_NODE, 0.0),
                        (ROAD_INVALID_NODE, 0.0),
                        (ROAD_INVALID_NODE, 0.0),
                    ],
                ),
                placed_node(
                    10.0,
                    [
                        (ROAD_INVALID_NODE, 0.0),
                        (ROAD_INVALID_NODE, 0.0),
                        (ROAD_INVALID_NODE, 0.0),
                        (ROAD_INVALID_NODE, 0.0),
                    ],
                ),
            ],
            links: vec![plain_link(0, 1)],
        };
        let mut out = Vec::new();
        assert_eq!(find_path(&graph, 0, 1, &mut out), Ok(false));
        assert_eq!(out, vec![0]);
    }

    #[test]
    fn output_clear_and_zone_boundaries_carry_their_entries() {
        let mut path = vec![10u16, 20, 30];
        clear_path_output(&mut path);
        assert!(path.is_empty());
        assert_eq!(CLEAR_OUTPUT_ENTRY, 0x55cb90);
        assert_eq!(ZONE_RESOLVE_ENTRY, 0x55be10);
    }

    fn plain_link(first: u16, second: u16) -> RoadLink {
        let mut link_bytes = [0u8; ROAD_LINK_SIZE];
        link_bytes[ROAD_LINK_ENDPOINT_OFFSET..ROAD_LINK_ENDPOINT_OFFSET + 2]
            .copy_from_slice(&first.to_le_bytes());
        link_bytes[ROAD_LINK_ENDPOINT_OFFSET + 2..ROAD_LINK_ENDPOINT_OFFSET + 4]
            .copy_from_slice(&second.to_le_bytes());
        RoadLink { bytes: link_bytes }
    }

    fn two_node_network() -> RoadNetwork {
        // Nodes at [0,0,0] and [3,4,0] with mutual lane 0; one link at the
        // origin with plain endpoints (0, 1).
        let mut second = placed_node(
            0.0,
            [
                (0, 2.0),
                (ROAD_INVALID_NODE, 0.0),
                (ROAD_INVALID_NODE, 0.0),
                (ROAD_INVALID_NODE, 0.0),
            ],
        );
        second.bytes[4..8].copy_from_slice(&4.0f32.to_le_bytes());
        RoadNetwork {
            nodes: vec![
                placed_node(
                    0.0,
                    [
                        (1, 2.0),
                        (ROAD_INVALID_NODE, 0.0),
                        (ROAD_INVALID_NODE, 0.0),
                        (ROAD_INVALID_NODE, 0.0),
                    ],
                ),
                second,
            ],
            links: vec![plain_link(0, 1)],
        }
    }

    #[test]
    fn edge_lane_selects_last_or_lightest_match() {
        let graph = line_graph();
        assert_eq!(edge_lane(&graph, 0, 1, true), Ok(Some(0)));
        assert_eq!(edge_lane(&graph, 0, 9, true), Ok(None));
        assert_eq!(
            edge_lane(&graph, 9, 1, true),
            Err(RoadWalkError::NodeOutOfRange { id: 9 })
        );
        // Two lanes to the same target: lightest wins only when preferred;
        // otherwise the last matching lane wins.
        let graph = RoadNetwork {
            nodes: vec![
                placed_node(
                    0.0,
                    [
                        (1, 2.0),
                        (1, 5.0),
                        (ROAD_INVALID_NODE, 0.0),
                        (ROAD_INVALID_NODE, 0.0),
                    ],
                ),
                placed_node(
                    0.0,
                    [
                        (ROAD_INVALID_NODE, 0.0),
                        (ROAD_INVALID_NODE, 0.0),
                        (ROAD_INVALID_NODE, 0.0),
                        (ROAD_INVALID_NODE, 0.0),
                    ],
                ),
            ],
            links: Vec::new(),
        };
        assert_eq!(edge_lane(&graph, 0, 1, true), Ok(Some(0)));
        assert_eq!(edge_lane(&graph, 0, 1, false), Ok(Some(1)));
    }

    #[test]
    fn edge_validate_pushes_start_and_reports_stop_kind() {
        let graph = line_graph();
        // Plain start: pushed, then walk ends immediately with true.
        let mut out = Vec::new();
        assert_eq!(edge_validate(&graph, 1, &mut out, 0xFFFF, 2), Ok(true));
        assert_eq!(out, vec![1]);
        // stop_done reached: false, output retains the chain.
        let mut out = Vec::new();
        assert_eq!(edge_validate(&graph, 0, &mut out, 0xFFFF, 0), Ok(false));
        assert_eq!(out, vec![0]);
        // stop_truncate clears before the push lands.
        let mut out = vec![9u16];
        assert_eq!(edge_validate(&graph, 0, &mut out, 0, 2), Ok(true));
        assert_eq!(out, vec![0]);
        // Invalid start ends early with true and empty output.
        let mut out = Vec::new();
        assert_eq!(
            edge_validate(&graph, ROAD_INVALID_NODE, &mut out, 0xFFFF, 0xFFFF),
            Ok(true)
        );
        assert!(out.is_empty());
    }

    #[test]
    fn resolve_zone_picks_the_closer_valid_side() {
        let graph = two_node_network();
        // fixup(0x8000 -> 0) = (0, 0.0); fixup(0x8000 -> 1) = (1, 25.0).
        // Both lanes exist; closer distance (side 0) wins without reverse.
        assert_eq!(resolve_zone(&graph, 0x8000, false), Ok((0, 0)));
        assert_eq!(resolve_zone(&graph, 0x8000, true), Ok((0, 0)));
        // Unknown link index is an explicit host error, never wild memory.
        assert_eq!(
            resolve_zone(&graph, 0x8001, false),
            Err(ZoneResolveError::Walk(RoadWalkError::LinkOutOfRange {
                id: 0x8001
            }))
        );
        // Empty graph has no link to resolve through.
        assert_eq!(
            resolve_zone(&line_graph(), 0x8000, false),
            Err(ZoneResolveError::Walk(RoadWalkError::LinkOutOfRange {
                id: 0x8000
            }))
        );
    }

    fn placed_node(x: f32, lanes: [(u16, f32); 4]) -> RoadNode {
        let mut bytes = [0u8; ROAD_NODE_SIZE];
        bytes[0..4].copy_from_slice(&x.to_le_bytes());
        for (lane, (neighbor, weight)) in lanes.iter().enumerate() {
            bytes[ROAD_LANE_NEIGHBOR_OFFSET + lane * ROAD_LANE_STRIDE
                ..ROAD_LANE_NEIGHBOR_OFFSET + lane * ROAD_LANE_STRIDE + 2]
                .copy_from_slice(&neighbor.to_le_bytes());
            bytes[ROAD_LANE_WEIGHT_OFFSET + lane * ROAD_LANE_STRIDE
                ..ROAD_LANE_WEIGHT_OFFSET + lane * ROAD_LANE_STRIDE + 4]
                .copy_from_slice(&weight.to_le_bytes());
        }
        // Fill remaining lanes as absent.
        for lane in lanes.len()..ROAD_LANES {
            bytes[ROAD_LANE_NEIGHBOR_OFFSET + lane * ROAD_LANE_STRIDE
                ..ROAD_LANE_NEIGHBOR_OFFSET + lane * ROAD_LANE_STRIDE + 2]
                .copy_from_slice(&ROAD_INVALID_NODE.to_le_bytes());
        }
        RoadNode { bytes }
    }

    fn line_graph() -> RoadNetwork {
        // 0 --1--> 1 --1--> 2, positions 0/10/30 on X for distances.
        RoadNetwork {
            nodes: vec![
                placed_node(
                    0.0,
                    [
                        (1, 1.0),
                        (ROAD_INVALID_NODE, 0.0),
                        (ROAD_INVALID_NODE, 0.0),
                        (ROAD_INVALID_NODE, 0.0),
                    ],
                ),
                placed_node(
                    10.0,
                    [
                        (2, 1.0),
                        (ROAD_INVALID_NODE, 0.0),
                        (ROAD_INVALID_NODE, 0.0),
                        (ROAD_INVALID_NODE, 0.0),
                    ],
                ),
                placed_node(
                    30.0,
                    [
                        (ROAD_INVALID_NODE, 0.0),
                        (ROAD_INVALID_NODE, 0.0),
                        (ROAD_INVALID_NODE, 0.0),
                        (ROAD_INVALID_NODE, 0.0),
                    ],
                ),
            ],
            links: Vec::new(),
        }
    }

    #[test]
    fn lane_accessors_read_evidence_slots() {
        let graph = line_graph();
        assert_eq!(graph.nodes[0].position(), [0.0, 0.0, 0.0]);
        assert_eq!(graph.nodes[1].lane_neighbor(0), 2);
        assert_eq!(graph.nodes[1].lane_weight(0), 1.0);
        assert_eq!(graph.nodes[2].lane_neighbor(3), ROAD_INVALID_NODE);
    }

    #[test]
    fn dijkstra_follows_lanes_to_the_target() {
        let graph = line_graph();
        assert_eq!(dijkstra_zones(&graph, 0, 2), Ok(vec![0, 1, 2]));
        assert_eq!(dijkstra_zones(&graph, 0, 1), Ok(vec![0, 1]));
        assert_eq!(
            dijkstra_zones(&graph, 2, 0),
            Err(RoadWalkError::Unreachable)
        );
        assert_eq!(
            dijkstra_zones(&graph, 1, 1),
            Err(RoadWalkError::Unreachable)
        );
        assert_eq!(
            dijkstra_zones(&graph, 0, 9),
            Err(RoadWalkError::Unreachable)
        );
        assert_eq!(
            dijkstra_zones(&RoadNetwork::default(), 0, 1),
            Err(RoadWalkError::Unreachable)
        );
    }

    #[test]
    fn fixup_accumulates_squared_distances_and_stops_at_plain_ids() {
        let graph = line_graph();
        // Plain `to` id: one segment 0 -> 1 (distance 100), terminal 1.
        assert_eq!(fixup_distance(&graph, 0, 1), Ok((1, 100.0)));
        assert_eq!(
            fixup_distance(&graph, 0, ROAD_INVALID_NODE),
            Ok((ROAD_INVALID_NODE, 0.0))
        );
        assert_eq!(
            fixup_distance(&graph, 0, 9),
            Err(RoadWalkError::NodeOutOfRange { id: 9 })
        );
    }

    #[test]
    fn emit_pushes_id_and_stops_at_plain_start() {
        let graph = line_graph();
        let mut out = Vec::new();
        emit_chain(&graph, 7, 1, &mut out, 0xFFFF, 0xFFFF).unwrap();
        assert_eq!(out, vec![7, 1]);
        let mut out = Vec::new();
        emit_chain(&graph, 7, ROAD_INVALID_NODE, &mut out, 0xFFFF, 0xFFFF).unwrap();
        assert_eq!(out, vec![7]);
        let mut out = Vec::new();
        emit_chain(&graph, 7, 1, &mut out, 1, 0xFFFF).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn encode_mirrors_savedata_layout_and_round_trips() {
        let bytes = fixture(2, 3, 0xaa, 0x55);
        let network = parse_road_network(&bytes).unwrap();
        let encoded = encode_road_network(&network);
        // Zero fills contain no near-base snap slots except where the fill
        // itself decodes near one; re-parsing is a fixed point regardless.
        let reparsed = parse_road_network(&encoded).unwrap();
        assert_eq!(reparsed, network);
        assert_eq!(encode_road_network(&reparsed), encoded);
        let empty = RoadNetwork::default();
        assert_eq!(
            encode_road_network(&empty),
            [2u32.to_le_bytes(), 0u32.to_le_bytes(), 0u32.to_le_bytes()].concat()
        );
    }

    #[test]
    fn length_helper_follows_native_thresholds() {
        assert_eq!(road_vector_length(0.0), 0.0);
        assert_eq!(road_vector_length(f32::from_bits(0x322b_cc76)), 0.0);
        assert_eq!(road_vector_length(1.0), 1.0);
        assert_eq!(road_vector_length(1.0 + 5e-9), 1.0);
        assert_eq!(road_vector_length(4.0), 2.0);
        assert_eq!(road_vector_length(2.0), 2.0f32.sqrt());
    }

    #[test]
    fn backref_reset_clears_both_slots() {
        let mut graph = two_node_network();
        // Poison the slots first: reset must clear regardless of content.
        graph.links[0].bytes[ROAD_LINK_BACKREF_OFFSET..ROAD_LINK_BACKREF_OFFSET + 4]
            .copy_from_slice(&[0x11, 0x22, 0x33, 0x44]);
        reset_link_backrefs(&mut graph);
        assert_eq!(
            &graph.links[0].bytes[ROAD_LINK_BACKREF_OFFSET..ROAD_LINK_BACKREF_OFFSET + 4],
            &[0xFF, 0xFF, 0xFF, 0xFF]
        );
        // Endpoints survive the reset untouched.
        assert_eq!(graph.links[0].endpoints(), (0, 1));
    }

    fn x_link(x: f32, endpoints: (u16, u16)) -> RoadLink {
        let mut bytes = [0u8; ROAD_LINK_SIZE];
        bytes[0..4].copy_from_slice(&x.to_le_bytes());
        bytes[ROAD_LINK_ENDPOINT_OFFSET..ROAD_LINK_ENDPOINT_OFFSET + 2]
            .copy_from_slice(&endpoints.0.to_le_bytes());
        bytes[ROAD_LINK_ENDPOINT_OFFSET + 2..ROAD_LINK_ENDPOINT_OFFSET + 4]
            .copy_from_slice(&endpoints.1.to_le_bytes());
        RoadLink { bytes }
    }

    #[test]
    fn sort_orders_by_x_and_remaps_zone_refs() {
        let mut graph = RoadNetwork {
            nodes: vec![placed_node(
                0.0,
                [
                    (ROAD_INVALID_NODE, 0.0),
                    (ROAD_INVALID_NODE, 0.0),
                    (ROAD_INVALID_NODE, 0.0),
                    (ROAD_INVALID_NODE, 0.0),
                ],
            )],
            links: vec![
                x_link(30.0, (0x8000, 0x8001)),
                x_link(10.0, (0x8001, 0x8002)),
                x_link(20.0, (5, 0x8000)),
            ],
        };
        // Node flag referencing the middle link follows it through the sort.
        graph.nodes[0].set_lane_flag(0, 0x8001);
        let report = sort_nodes_by_x(&mut graph);
        assert_eq!(report.swaps, 2);
        let xs: Vec<f32> = graph
            .links
            .iter()
            .map(|link| f32::from_le_bytes(link.bytes[0..4].try_into().unwrap()))
            .collect();
        assert_eq!(xs, vec![10.0, 20.0, 30.0]);
        // Endpoints and flags tracked the records through both swaps.
        assert_eq!(graph.links[0].endpoints(), (0x8000, 0x8001));
        assert_eq!(graph.links[1].endpoints(), (5, 0x8002));
        assert_eq!(graph.links[2].endpoints(), (0x8002, 0x8000));
        assert_eq!(graph.nodes[0].lane_flag(0), 0x8000);
    }

    #[test]
    fn sort_keeps_incumbent_on_nan_and_equal() {
        let mut graph = RoadNetwork {
            nodes: Vec::new(),
            links: vec![
                x_link(1.0, (0, 1)),
                x_link(f32::NAN, (2, 3)),
                x_link(1.0, (4, 5)),
            ],
        };
        let report = sort_nodes_by_x(&mut graph);
        // NaN never wins: first record stays first, equal keeps order.
        assert_eq!(report.swaps, 0);
        assert_eq!(graph.links[0].endpoints(), (0, 1));
        assert!(f32::from_le_bytes(graph.links[1].bytes[0..4].try_into().unwrap()).is_nan());
    }

    fn walk_network() -> RoadNetwork {
        // Node 0 at the origin with lane 0 flagged at link 0; node 1 at
        // [3,4,0]; link 0 at the origin with endpoints (1, 0xFFFF).
        let mut first = placed_node(
            0.0,
            [
                (1, 0.0),
                (ROAD_INVALID_NODE, 0.0),
                (ROAD_INVALID_NODE, 0.0),
                (ROAD_INVALID_NODE, 0.0),
            ],
        );
        first.set_lane_flag(0, 0x8000);
        let mut second = placed_node(
            0.0,
            [
                (ROAD_INVALID_NODE, 0.0),
                (ROAD_INVALID_NODE, 0.0),
                (ROAD_INVALID_NODE, 0.0),
                (ROAD_INVALID_NODE, 0.0),
            ],
        );
        second.bytes[0..12].copy_from_slice(&[3.0f32, 4.0, 0.0].map(f32::to_le_bytes).concat());
        let mut link_bytes = [0u8; ROAD_LINK_SIZE];
        link_bytes[ROAD_LINK_ENDPOINT_OFFSET..ROAD_LINK_ENDPOINT_OFFSET + 2]
            .copy_from_slice(&1u16.to_le_bytes());
        link_bytes[ROAD_LINK_ENDPOINT_OFFSET + 2..ROAD_LINK_ENDPOINT_OFFSET + 4]
            .copy_from_slice(&ROAD_INVALID_NODE.to_le_bytes());
        RoadNetwork {
            nodes: vec![first, second],
            links: vec![RoadLink { bytes: link_bytes }],
        }
    }

    #[test]
    fn walk_gate_skips_absent_lanes() {
        let mut graph = line_graph();
        // line_graph lanes carry no flags (zeroed +0x14 words read as 0).
        // Flag 0 is a plain link index; link 0 does not exist: host error,
        // never wild memory.
        assert_eq!(
            recompile_lane_walk(&mut graph, 0, 1, 7),
            Ok(LaneWalkOutcome::Skipped)
        );
        // Lane 0 of node 0 has a neighbor but flag 0 with no links at all.
        assert!(matches!(
            recompile_lane_walk(&mut graph, 0, 0, 7),
            Err(RoadWalkError::LinkOutOfRange { .. })
        ));
        assert_eq!(
            recompile_lane_walk(&mut graph, 9, 0, 7),
            Err(RoadWalkError::NodeOutOfRange { id: 9 })
        );
    }

    #[test]
    fn walk_accumulates_true_lengths_and_backpatches() {
        let mut graph = walk_network();
        let outcome = recompile_lane_walk(&mut graph, 0, 0, 41).unwrap();
        // |node0 − link0| = 0, then link0 → node1 = 5.
        assert_eq!(
            outcome,
            LaneWalkOutcome::Done {
                terminal: 1,
                weight: 5.0,
                visited: 1,
            }
        );
        assert_eq!(graph.nodes[0].lane_weight(0), 5.0);
        assert_eq!(graph.nodes[0].lane_neighbor(0), 1);
        assert_eq!(graph.links[0].backrefs(), (41, 1));
    }
}
