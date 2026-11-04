# Branch hygiene + SortNodes port (2026-11-04)

## Branch triage

Remote `ls-remote` showed 13 non-`rust-engine` branches (the checkout is
single-branch, so `branch -r` hid them until an explicit fetch). Ancestor
check against `origin/rust-engine`:

- 12 merged → remote branches deleted: `docs/nine-parity-followups-2026-11-01`,
  `feat/general-companion-animation`, `feat/live-script-lookup-registers`,
  `feat/mission-program-world`, `feat/owned-script-lookup-execution`,
  `feat/owned-script-name-parameters`, `feat/owned-vm-pair-parameters-2026-11-03`,
  `feat/retained-session-program-world`, `feat/traffic-generator-source-owner`,
  `fix/canonical-scene2-crash-owners`, `fix/empty-model-cache-hierarchy`,
  `fix/persistent-scene-lighting`.
- 1 unmerged → KEPT: `feat/owned-script-live-lookups` (tip 6be7397, Oct 30,
  "keep draft", no executed validation). Its FINDFRAME/FINDACTOR work was
  completed through reviewed PRs #59/#60, but the tip is not an ancestor of
  merged work and is only reachable from its own branch, so deleting it
  would destroy unique history. Decision on it belongs to the user.

Local tracking refs for the deleted remotes removed with `branch -rd`;
`mafia-re` `pr3` mirror removed earlier (upstream PR #3 still OPEN).
`rust-engine` pushed and in sync.

## SortNodesByX port (`sort_nodes_by_x`)

Rizin ASM (0x55c890–`ret` 0x55ca94) plus the gate region re-read gives the
complete mechanism: selection sort of the link array by position-X with
the exact strict-less update (`fld`/`fcomp`/`fnstsw`/`test ah,5`/`jp` —
unordered/NaN and equal keep the incumbent); each swap exchanges the
24-byte records and remaps zone-marked refs `i|0x8000` ↔ `j|0x8000` in
link endpoint slots (+0x10/+0x12, first remap loop over 2 u16s per link)
and node lane-flag slots (+0x14 + 2 bytes × lane, second loop over 4
u16s per node). The gate rule is confirmed: lane `k` needs flag
+0x14+2k and neighbor +0x1C+0x34k both non-`0xFFFF` (Rizin: nodebase+0x1A
walking down by 2 beside the 0x34 lane stride; H1 counts match:
+0x14/+0x16 never `0xFFFF`, +0x18 has 35, +0x1A has 194). The `0x55caa0`
entry guard is skipped with documentation (unobservable on any input:
empty sorts are no-ops either way).

Supporting pieces: `road_vector_length` (native `length()` thresholds
`0x322bcc77`/1e-8, shared with vehicle code), `reset_link_backrefs`
(Recompile phase A), `lane_flag` accessor/setter.

## Verification

- Synthetic: ordered sort with dual remap (2 swaps traced by hand, then
  corrected by the test itself when the trace was wrong), NaN/equal
  incumbent stability.
- Shipped mise01/road.bin: **0 swaps, byte-identical output** — the port
  is a fixed point on mission data, so loading behavior is unchanged.
- Full lib 1924 passed / 1 pre-existing `ph_thing_ai` failure; clippy clean.

## Open next

Recompile walk + lane permutation (all three pseudocode pages retained;
empirical result that shipped weights are chain-accumulated, not direct
distances, and lanes are not weight-ordered, so the port must reproduce
the walk, not assume sortedness), `MergeData` boundary, base-cache `Open`
scene integration.
