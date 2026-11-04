# Recompile walk analysis + SortNodes port (2026-11-04)

## RecompileDistances 0x55a620: analysis complete, port staged

Full evidence: REA Ghidra `ev_69eb9075…` (869/96/0/3, all three
pseudocode pages ≈ 26 KB) plus targeted Rizin reads of the gate
(0x55a6dc), walk (0x55a7d9), terminal (0x55a8f4) and write-back
(0x55a949) regions.

Phases: (A) reset link +0x14/+0x16 to `0xFFFF` → ported as
`reset_link_backrefs`, tested. (B) per node (reverse), per lane
(reverse 3→0): gate = flag +0x14+2·lane AND neighbor +0x1C+0x34·lane
both non-`0xFFFF` — confirmed in raw ASM (nodebase+0x1A walking down
by 2 beside the 0x34 lane stride; H1 counts match the file:
+0x14/+0x16 never absent, +0x18 ×35, +0x1A ×194). First segment is
|node − link[flag&0x7FFF]| through the shared `length()` helper
(`0x322bcc77`/1e-8/1.0, data refs 1.0 at 0x623238 and 1e-8 at
0x6232e8) → ported as `road_vector_length`, tested. (C) lane-block
(0x34) permutation between +0x1C/+0x50/+0x84/+0xB8 with u16s at
+0x14/+0x16/+0x18/+0x1A, ordered by accumulated length.

Deliberately NOT ported yet: the chain-walk's per-step backpatch value
(`dx` from `[esp+0x1c]`, origin unresolved in the truncated display),
visited-list dedup semantics of the 0x4824c0 pushes, and the permutation
choreography. A wrong port passing fixed-point by luck is worse than an
explicit boundary. Decisive experiment already ran: shipped lane weights
are chain-accumulated (0/675 equal direct distances), so the file is in
post-recompile state and mission loading does not depend on this port.

## SortNodesByX 0x55c890: ported and proven

Rizin ASM 0x55c890–`ret` 0x55ca94: selection sort of links by
position-X with the exact strict-less update (`fld`/`fcomp`/`fnstsw`/
`test ah,5`/`jp` — NaN and equal keep the incumbent); each swap
exchanges 24-byte records and remaps `i|0x8000` ↔ `j|0x8000` in link
endpoints (+0x10/+0x12) and node lane flags (+0x14+2·lane). The `0x55caa0`
entry guard is skipped with documentation (unobservable either way).
`sort_nodes_by_x` + `SortReport{swaps}` + 4 tests, including a hand-traced
double swap (the test caught my trace error, code was right) and
NaN/equal stability. Shipped data: **0 swaps, byte-identical**.

## Validation

Full lib 1924 passed / 1 pre-existing `ph_thing_ai` failure; clippy
clean; `encode(parse(file))` and `sort(file)` both byte-identical on the
shipped 81,032 bytes.
