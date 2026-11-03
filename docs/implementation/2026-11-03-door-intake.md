# Door actor intake (kind 6) — 2026-11-03

Reference code: [mafia-re C_door.h](https://github.com/Marvisak/mafia-re/blob/d0fbd6dba9ac5b31cabd6661c9bb2b855eab5496/game/actors/C_door.h)
with C_mission::CreateActor and Mission.cpp actor arms. Tested against the
local Game.exe SHA-256
08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27
(constructor VA 0x439290, Init 0x4393a0, LoadData 0x43b030,
vtable VA 0x623b20, 0x1b8 bytes; LoadData slot 20 and Init slot 18 confirmed
against the detector/bridge vtable layout).

## Implemented scope

- Owned `DoorState`/`DoorSeed` (`door_actor.rs`): the exact constructor body
  in dependency order (zeroed DWORD ranges 0x78–0xbf/0x190/0x198–0x1b7, timer
  words 0x3b83126f at +0x128/+0x12c, flag bytes, float word 0x3fc90fdb at
  +0x120). The three sound names (`d_woodop.wav`, `d_woodcl.wav`,
  `d_woodlo.wav`) are written to shared world storage, matching the native
  repeated strcpy into static buffers; later constructions overwrite earlier
  ones.
- `load_door_data`: version-word dispatch (5/6/7 populate ascending field
  prefixes, 8 the full flag/field/name set, others consume only the version
  word). Every field read is individually guarded: truncated tails keep prior
  values field by field while later narrow reads still land, exactly like the
  native guard chain; a missing version word consumes nothing. Returns
  consumed payload bytes. The MISE01 door (`dvere_frontX`) is version 8 with a
  complete 70-byte payload (4 + 17 field bytes + 48 name bytes + 1 flag byte).
- Door Init is the base Init with no derived tail (decompile is a direct
  forward), so the generic intake init covers it; release/destroy/delete mirror
  the bridge lifecycle; `allocate_door` (0x1b8), stream kind-6 LoadData and
  release arms, content-loading allocation and disposal arms.
- Four pre-existing tests used kind 6 as their unsupported-kind specimen and
  now use kind 9 (dyn_object, still unsupported).

## Hard limits and documented deviations

- Door Open/Close/Lock effects and sound playback are unimplemented; the
  retained fields and shared sound names await their effect owner.
- The version-word-missing path consumes nothing and succeeds; the original
  switches on uninitialized stack garbage there (almost surely a no-op), which
  is not reproducible. Independent review outstanding.

## Verification

```sh
cargo fmt --all -- --check
cargo test --locked -p lhr-engine --all-features --lib -- door_
cargo test --locked -p lhr-engine --all-features --lib
cargo clippy --locked --all-targets --all-features -- -D warnings
```

Constructor/sound/version unit tests, world tests (factory bytes and kind,
full version-8 load with a 70-byte synthetic payload, older-version prefixes,
unknown/empty/truncated inputs, release refcount and deletion) and a session
test (version-8 record loads, cursor advances, disposal completes). Full
suite: 1815 passed, 11 ignored, 1 pre-existing ARM64-only `ph_thing_ai`
failure. MISE01 probe: no kind-6 error; frontier unchanged at kind 25
(pumper precedes the door in file order). No fresh GPU captures on macOS
ARM64; Linux software-Vulkan captures stay current.
