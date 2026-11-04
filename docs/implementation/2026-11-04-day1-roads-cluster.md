# Day 1: road cluster reconstruction (2026-11-04)

Reconstructs `road_web.cpp` from memory-segment ASM evidence upward, one
function at a time, each rewritten as a logical readable Rust owner with
synthetic tests. Reference: Game.exe SHA-256
`08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27`;
mafia-re `d0fbd6d` file 70 (all Bodied functions there are stubs).

## Recovered functions

| PC entry | Size | Evidence | Status |
| --- | --- | --- | --- |
| `SaveData` 0x559d40 | ~0x11A bytes, `ret 4`, true | Rizin `pd` 0x559d40–0x559e5a (`/tmp/rea-savedata-asm.txt`, outside Git) | Ported as `encode_road_network` |
| `LoadData` 0x559e60 | 324 insns / 55 blocks | REA Ghidra `ev_4ac5d63c…` (+ 4,746 pseudocode units) | Ported earlier as `parse_road_network` |
| `FindPath` 0x55c4e0 | 292 / 47, 3 params, 9 callees | REA Ghidra `ev_755c7588…` (+ 4,857 units) | Contract + sentinel/zone rules; search open |
| output clear 0x55cb90 | 20 / 5, 0 callees, 2 callers | REA Ghidra `ev_97ead2f7…` | Ported as `clear_path_output` |
| zone resolve 0x55be10 | 132 / 18, 3 params, 2 callees | REA Ghidra `ev_abb271eb…` | Ported as `resolve_zone` |
| emit 0x55bc10 | 63 / 12, 5 params, 1 callee | REA Ghidra `ev_6ceec1cd…` (+ 1,082 units) | Ported as `emit_chain` |
| fixup 0x55bcd0 | 97 / 13, 3 params, 0 callees | REA Ghidra `ev_77cb6fe3…` (+ 1,213 units) | Ported as `fixup_distance` |
| zone search 0x55bf60 | 380 / 55, 3 params, 7 callees | REA Ghidra `ev_23ee93c7…` (+ 6,114 units) | Ported as `dijkstra_zones` |
| edge weight 0x55b5d0 | 104 / 23, 3 params, 0 callees | REA Ghidra `ev_b3e004ff…` (+ 1,730 units) | Ported as `edge_lane` |
| edge validate 0x55c440 | 62 / 13, 5 params, 1 callee | REA Ghidra `ev_b1ea4914…` (+ 970 units) | Ported as `edge_validate` |
| elem ctor 0x55c430 | 4 insns, 1 block | REA Ghidra `ev_bf43bc2b…` | Ported as `ROAD_UNREACHED_DIST_BITS` |
| `RecompileDistances` 0x55a620 | 869 / 96, 0 params, 3 callees | REA Ghidra `ev_69eb9075…` (1.6 MB bundle) | Open, next |
| `SortNodesByX` 0x55c890 | selection sort + remaps | Rizin ASM (selection rule + swap/remap decoded) | Open, next |
| `MergeData` 0x55a210 | — | No Ghidra procedure (PC-only, likely folded) | Boundary with entry |

## Findings that changed the model

- Chunk flags from SaveData ASM: +0x10 is open, +0x18 is **write-mode**
  (writes fire only when set; reads only when clear). This corrects the
  earlier read-side note.
- `SaveData` member words: node begin/end at `this`+8/+0xC, link begin/end
  at +0x18/+0x1C; counts via division magics (`0x22b63cbf`/`sar 5` for
  /236, `0x2aaaaaab`/`sar 2` for /24); bulk sizes via lea chains.
- Node 0xEC records: position `S_vector` at +0x00 (fixup reads
  `pfVar[0..2]` from both arrays), 4 Dijkstra lanes stride 0x34 with u16
  neighbor at +0x1C and f32 weight at +0x20. Shipped lane neighbors are
  plain node ids 0..228 (`0xFFFF` = absent).
- Link 0x18 records: position at +0x00, speed float at +0x0C (shipped
  values are speed limits in m/s: 5.56/6.94/8.33/9.72/11.11/13.3/13.89),
  plain node indices at +0x14/+0x16, zone-marked endpoint pair at
  +0x10/+0x12. Zone bit `0x8000` selects the link array; plain ids select
  nodes — confirmed by fixup addressing on both sides.
- `FindPath` flag convention decoded: 0x55b5d0 third arg `-1` means
  last-match, anything else means minimum-weight.
- Dijkstra element init is `pred = 0xFFFF`, `dist = 0x501502f9`
  (≈ 8.59e9), not infinity; unreached test reads the predecessor.
- `SortNodesByX` is a selection sort of links by position-X with exact
  strict-less update (NaN keeps incumbent, provable from the
  `fcomp`/`test ah,5`/`jp` sequence) plus zone-ref remaps in link
  endpoints and node lane slots on swap. Shipped mise01 links are already
  sorted with no NaNs, so it is a fixed point there (verified: 0
  violations over 1,124 links).

## Verification on shipped mise01/road.bin

- `encode(parse(file))` is byte-identical to the 81,032 shipped bytes.
- Dijkstra sample paths terminate and validate (consecutive lane
  adjacency): 0→228 in 14 hops (symmetric back), 10→200 in 11, plus
  unit pairs; `resolve_zone(0x8000)` terminates with zone 210 side 0.
- Full lib 1906+ passed / 1 pre-existing `ph_thing_ai` failure.

## Open (next)

`RecompileDistances` 0x55a620 (evidence retained, 869 insns — the
heaviest remaining), `SortNodesByX` port with remap loops,
`MergeData` 0x55a210 boundary, then `FindPath` end-to-end assembly over
`resolve_zone` + `dijkstra_zones` + `edge_validate`. Batch-3/4 REA
evidence IDs are cited in code comments; bundles stay outside Git.
