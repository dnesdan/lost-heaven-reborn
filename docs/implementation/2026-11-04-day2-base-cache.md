# Day 2: base-cache Open evidence + owner (2026-11-04)

Branches are clean (`pr69` merged-then-deleted, mafia-re `pr3` mirror
deleted with upstream PR #3 still OPEN and re-fetchable) and `rust-engine`
is pushed. This package lands the base-cache owner with session lifecycle
plus the complete `Open` evidence set; the scene-integrated `Open` port
remains open with exact entry points.

## REA evidence: `C_cache_base_block::Open` 0x4013f0

REA Ghidra (`ev_5792b50b…`, 680 instructions / 114 blocks / 2 params /
24 callees / 7 unresolved) plus all three pseudocode pages, corroborated
by Rizin byte reads of the string table:

- `Close()` + `PrepareScene()` first, then the root chunk must be kind
  500 with version u32 1 (else return 1; non-chunk input returns 2) —
  exactly the walk the existing `parse_city_cache` already implements
  (500 → 1000 → 2000).
- Part chunks (1000): length-prefixed name with grown buffer, model load
  through the scene vtable, child-frame enumeration with sound
  collection; frame chunks (2000): model name, then placement fields.
- `0rek23.i3d` (bytes at 0x62e460) is overwritten in place with
  `0rek17.i3d` (bytes at 0x62e454) after a case-insensitive compare —
  the substitution `neighborhood.rs` used to refuse.
- Unknown model kind shows the "Chybí model" box (0x62e430) with
  `model "%s" not found.` (0x62e43c) and returns 4; model load failure
  returns 3.
- Success epilogue: `unknown_20 = 0`, `open = true`, `unknown_28 =
  0x0FFFFFFF`, plus the 12 bytes from `FUN_00402a30(0, 0xc47a0000, 0)`
  into +0x2C/+0x30/+0x34. (This corrects the constructor-only picture:
  `unknown_28` is 0 at construction and `0x0FFFFFFF` after a successful
  `Open`; the object extends to at least 0x38.)

## Ported now (`city_base_cache.rs`)

- Layouts from mafia-re `m_cache.h` (PS2-attributed, PC sizes): block
  0x74, placement 0x44 (set index u16 +0x2), set 0x14, element 8 bytes.
  Native frame words are presence flags, never host pointers.
- Constructor 0x401070 (`open = false`, `unknown_28 = 0`, `unknown_20`
  retained) and `Close` 0x4011f0 (open first, blocks, reverse model-set
  teardown with a counted report) with tests.
- `rewrite_unsupported_cache_model` (case-insensitive 0rek23→0rek17)
  with tests; `neighborhood.rs` now substitutes instead of aborting —
  mise01 has 3 such placements and `models/0rek17.4ds` is present.
- `BaseCacheOpenOutcome` 0..4 return codes, `apply_open_success` epilogue
  (markers + 12 opaque bytes) with tests.
- Session lifecycle: `CityBaseCache::new(0)` owned by
  `MissionLoadSession` (native `.bss` zero at 0x63d268), `Close` in
  `discard_inner`, `base_blocks`/`base_model_sets` in owned counts,
  probe line (`Base cache: open=false blocks=0 model_sets=0` until `Open`
  lands).

## Open (scene-integrated port: next)

Chunk walk + version gates + name buffers + per-part model loads through
the model cache into frames + child enumeration + sound/light/sector
wiring + the epilogue above. Needs the model-cache/frame owners and the
`FUN_00402a30` helper; the 24-callee list in the retained bundle scopes
it. Validation: full lib 1920 passed / 1 pre-existing `ph_thing_ai`
failure; probe disposal fully zero including the new counters.
