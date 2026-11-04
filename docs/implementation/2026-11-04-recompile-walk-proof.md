# Recompile walk port: seed resolution + byte-exact proof (2026-11-04)

## The seed breakthrough

The walk's previous-seed register (`bx`) read `[esp+0x10]`, which also
feeds the gate's byte-offset math — suggesting the node byte offset
(236/node). But that yields terminal 1 for node1/lane0 while the file
carries 0 with weight 95.97. Rizin re-read of 0x55a72b–0x55a7ff shows
`ebx` is reassigned at 0x55a7c7 from `[esp+0x54]`, which took the
`[esp+0x1c]` forward node index: the seed is the **node forward index**
(1 for node 1), and `ep0(1) == seed(1)` takes the alternate endpoint
0x81f0 on the first select. Chain 449 → 496 → node 0 totals
8.59 + 76.5 + 10.9 = 95.99 ≈ file 95.9665 (float rounding): the walk
reaches terminal 0 exactly as shipped.

## What the port covers (`recompile_lane_walk`)

Gate (flag + neighbor non-`0xFFFF`), first segment |node − link[flag]|
via `road_vector_length`, per-step link+0x14 backpatch with the origin
forward index, visited pushes (plain `Vec`; 0x4824c0 REA `ev_4b7b2e7a…`
is a generic insert helper with 35 callers — no dedup, and duplicates
rewrite the same terminal idempotently), endpoint selection with the
forward-index seed, plain-node final segments, loop-back/absent
terminals, lane weight (+0x20) / terminal (+0x1C) writes, visited
+0x16 backpatch, reverse node/lane traversal order, step cap with
`Cycle` where native would not return.

## Proof on shipped mise01/road.bin

`recompute_lane_lengths` (reset + all 675 walks, 241 skips) reproduces
the 81,032 file bytes **exactly**. Every micro-decision above is
validated by this fixed point; a wrong turn anywhere (seed, select
rule, accumulation, backpatch values, order) diverges loudly.

## Angular permutation: evidence reviewed, port open

Phase C computes per-lane ANGLES (`FUN_00408490()` float10 result plus
π, local_18c/188/190) and conditionally rotates 0x34 lane blocks among
+0x1C/+0x50/+0x84/+0xB8 with u16 permutes at +0x14/+0x16/+0x18/+0x1A.
Discriminator: node 201 lanes [d1, FFFF, ca, FFFF] — only 1 of 229
nodes with a non-trailing absent lane. The trigger condition and move
topology need one more targeted pass; the walk port above is unaffected
(the permutation consumes walk outputs, never its inputs). `MergeData`
0x55a210 stays a boundary (no Ghidra procedure).
