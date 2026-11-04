# Day 1 completion: FindPath assembly (2026-11-04)

Assembles `C_roads::FindPath` (PC 0x55c4e0) end-to-end from the ported
pieces. All REA batches launched for day 1 are consumed; evidence bundles
stay outside Git with IDs cited in code.

## Cluster status

| PC entry | Evidence | Port |
| --- | --- | --- |
| `SaveData` 0x559d40 | Rizin ASM 0x559d40–0x559e5a | `encode_road_network` (byte-identical round-trip) |
| `LoadData` 0x559e60 | REA `ev_4ac5d63c…` | `parse_road_network` (earlier) |
| output clear 0x55cb90 | REA `ev_97ead2f7…` (20/5/0) | `clear_path_output` |
| zone resolve 0x55be10 | REA `ev_abb271eb…` (132/18/3/2) | `resolve_zone` |
| emit 0x55bc10 | REA `ev_6ceec1cd…` (63/12/5/1) | `emit_chain` |
| fixup 0x55bcd0 | REA `ev_77cb6fe3…` (97/13/3/0) | `fixup_distance` |
| zone search 0x55bf60 | REA `ev_23ee93c7…` (380/55/3/7) | `dijkstra_zones` |
| edge weight 0x55b5d0 | REA `ev_b3e004ff…` (104/23/3/0) | `edge_lane` |
| edge validate 0x55c440 | REA `ev_b1ea4914…` (62/13/5/1) | `edge_validate` |
| elem ctor 0x55c430 | REA `ev_bf43bc2b…` (4 insns) | `ROAD_UNREACHED_DIST_BITS` |
| `FindPath` 0x55c4e0 | REA `ev_755c7588…` (292/47/3/9) | `find_path` (this doc) |
| `RecompileDistances` 0x55a620 | REA `ev_69eb9075…` (869/96/0/3, 1.6 MB) | Open, analyzed below |
| `SortNodesByX` 0x55c890 | Rizin ASM (selection + remaps) | Open, analyzed below |
| `MergeData` 0x55a210 | No Ghidra procedure (folded) | Boundary with entry |

## FindPath order (ported)

Sentinel guard → output clear → zone resolution of zoned sides
(`Unreachable` ends the query) → plain-`from` push or chain emit →
cross-side fixup unification (collapses to one zone, `to_side` becomes
whether the old side was 0) → Dijkstra plus per-pair lane/validate walk →
non-empty tail check ending in `to` → `to`-zoned reversed-segment tail.
`true` is native 1, `false` native 0. Two deliberate host projections:
`Unreachable` maps to `false` (native early-outs), while out-of-range and
cycle errors propagate instead of reading wild memory; the unification
fixups run only when both masked link indices are live (on shipped data
they always are; the native order is preserved).

## RecompileDistances analysis (for the port)

Three phases recovered: (1) reset link +0x14/+0x16 back-refs to `0xFFFF`;
(2) per node lane walk accumulating true Euclidean lengths with the
`length()` helper semantics (`< 0x322bcc77` → 0, `|x − 1| < 1e-8` → 1,
else sqrt), storing weights at lane+0x20, terminals at lane+0x1C, and
backpatching link +0x16 entries; (3) lane-block (0x34) permutation between
+0x1C/+0x50/+0x84/+0xB8 with u16s at +0x14/+0x16/+0x18/+0x1A, ordered by
accumulated length. Decisive experiment: shipped lane weights do NOT equal
direct Euclidean distances (0/675 exact, up to 57% off), so the file is in
post-recompile state and mission loading does not depend on this port.

## SortNodesByX analysis (for the port)

Selection sort of links by position-X with the exact strict-less update
rule (provable from the `fcomp`/`test ah,5`/`jp` flag sequence; NaN keeps
the incumbent), swapping 24-byte records via a temp buffer plus
zone-marked ref remaps in link endpoints (+0x10/+0x12) and node lane
slots. Shipped links are already sorted with no NaNs (0 violations over
1,124), so it is a fixed point on mission data.

## Verification on shipped mise01/road.bin

- Plain pairs: 0→228 len 14, 10→200 len 11, 100→101 len 2, 5→5 len 1.
- Zoned pairs: 0x8000→100 len 30, 100→0x8001 len 31, 0x8000→0x8001
  len 1 — all ending at `to`.
- Sentinel returns `false` with the caller's prefix retained.
- Full lib 1915 passed / 1 pre-existing `ph_thing_ai` failure.
