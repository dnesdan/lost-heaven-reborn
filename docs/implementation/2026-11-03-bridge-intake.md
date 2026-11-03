# Bridge actor intake (kind 20) — 2026-11-03

Reference code: [mafia-re C_bridge.h](https://github.com/Marvisak/mafia-re/blob/d0fbd6dba9ac5b31cabd6661c9bb2b855eab5496/game/actors/C_bridge.h)
with C_mission::CreateActor and Mission.cpp actor arms; Game.exe vtable layout
verified by direct PE section mapping. Tested against the local Game.exe SHA-256
08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27
(constructor VA 0x567d10, Init 0x567e70, LoadData 0x56a6b0, ShutDown 0x56a7b0,
vtable VA 0x625588, 0x194 bytes; LoadData slot 20 confirmed against the
detector vtable holding 0x432b20 at the same slot).

## Implemented scope

- Owned `BridgeState`/`BridgeSeed` (`bridge_actor.rs`): the exact constructor
  body in dependency order (identity-derived control bytes at +0x1c/+0x20,
  zeroed ranges, the 0x3f5f66f3 flag word, timer words 120000/4000/30000/
  10000/20000, flag words 0x71–0x74/0xb6/0xb5, kind-7 word at +0x138).
  Original DWORD-slot indexes are byte offsets (each times four); the vtable
  store and kind marker live on the base actor owner in `construct_bridge`.
- `load_bridge_data`: the original reads no scene2 payload — it opens
  `tables\parnik.def` into shared storage (capstone-verified: push of the
  path string, a dword plus 0x2a0-byte read). The session reads the file from
  the installation on this path; an absent table fails closed, matching the
  native failed open. Zero payload bytes are consumed; the record is skipped
  by its declared extent, as for kind 1.
- `init_bridge_linked`: the Init tail after the base init — sibling flap
  frame resolution (exact name copy with last byte incremented, through the
  enclosing scene map into the +0x90 handle), the model-name L-flag at +0x140
  (names starting with L skip the shared sector refresh), and the sector
  refresh (two nine-word rows to 0xffff, eight cleared words), with offsets
  and values retained exactly. A missing sibling fails closed like the native
  null FindFrame result. No new backend signatures: the session already
  threads the installation, and the tail runs in a scoped borrow after the
  frames borrow ends.
- Release/destroy/delete wrappers mirroring the railway lifecycle, `Bridge`
  vtable arm in `create_actor` (0x194-byte `allocate_bridge`), disposal arm in
  content loading, and the factory-classifier test update (kind 20 admitted).
- Empty-payload guard fix: the 0xae24 arm previously skipped all
  empty-payload records except kind 18, so bridge LoadData never ran for the
  actual empty records; kind 20 now enters the arm like kind 18.

## Hard limits and documented deviations

- ShutDown (0x56a7b0) is not implemented: only the +0x13c target-flag byte
  inspector exists. The +0xb4 collision-link word selects a game-owned
  bridge-collision refresh with no backend. No accepted-function credit is
  claimed for ShutDown.
- The sibling lookup uses arena frame names for the native `model.frame`
  composition; both spell the frame's own name here. The `m_pName =
  "model.frame"` / `m_pOrigName` distinction from LS3DF is not modeled.
- Sector-refresh values are retained exactly but not interpreted; the game
  globals (DAT_00646f80 region) are per-world state here, not shared globals.
- The ferry table bytes stay opaque (a 680-byte `tables/parnik.def` in MISE01);
  no extractor interprets them yet.
- Entity command decode (0x1000+ IDs, DecodeParams shapes) and the remaining
  intake kinds (door 6, traffic_car 12, pumper 25, box_new 35) stay open; the
  MISE01 frontier is now kind 25. Independent review outstanding (no delegated
  reviewers available).

## Verification

```sh
cargo fmt --all -- --check
cargo test --locked -p lhr-engine --all-features --lib -- bridge_
cargo test --locked -p lhr-engine --all-features --lib
cargo clippy --locked --all-targets --all-features -- -D warnings
```

Six constructor/helper/table/sector unit tests, four world tests (factory
bytes and kind, LoadData table gating, linked init handle/flag/sectors,
release refcount and deletion) plus the factory-classifier update, and one
session test (linked `br01`→`br02` flap, unlinked `lone` release, cursor
advance, disposal). Full suite: 1795 passed, 11 ignored, 1 pre-existing
ARM64-only `ph_thing_ai` friction failure (`NumericDomain`, untouched since
PR #36, shares no code with this change). Original-data probe: MISE01 load
frontier moves from kind 20 (cursor 3814295) to kind 25 (cursor 3815824) with
complete disposal. No fresh GPU captures on macOS ARM64: headless Metal
mission capture stalls without output (wgpu device init without a surface);
Linux software-Vulkan captures stay current.
