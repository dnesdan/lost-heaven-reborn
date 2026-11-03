# Box actor intake (kind 35) — 2026-11-03

Reference code: [mafia-re C_box_new.h](https://github.com/Marvisak/mafia-re/blob/d0fbd6dba9ac5b31cabd6661c9bb2b855eab5496/game/actors/C_box_new.h)
with C_mission::CreateActor and Mission.cpp actor arms. Tested against the
local Game.exe SHA-256
08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27
(constructor VA 0x5674f0, Init 0x567540, LoadData 0x567730,
vtable VA 0x6254f0, 0x3a8 bytes; LoadData slot 20 and Init slot 18 confirmed
against the detector/bridge/door/traffic/pumper vtable layout).

## Implemented scope

- Owned `BoxNewState`/`BoxNewSeed` (`box_new_actor.rs`): the constructor
  stores no derived words (vtable and kind marker live on the base owner),
  so allocation passes through untouched.
- `init_box_volume`: the Init tail after the base init — six words with the
  volume-selected density (volumes strictly inside (0.01, 1000.0) scale
  +0x398 by 50, everything else including NaN keeps 10.0; NaN comparisons
  stay false on both sides like the native x87 compares). The caller
  refreshes hierarchy bounds through the arena (`hr_bound_volume`, which
  mirrors the native 0x40-flag UpdateHRBoundVol plus error propagation);
  unresolvable bounds fail closed like a refused base init. The
  UpdateWMatrix/GetScale refreshes feed no stored word and have no call here.
- `load_box_new_data`: flag dispatch (variant 0 reads the six words plus
  +0x3a0, variant 1 those plus +0x348, variant 2 additionally +0x34c; other
  flags consume only the flag byte), every field read individually guarded,
  with the trailing density clamp on all paths (above 100000.0 pins to
  100000.0, below 0.01 pins to 0.01, NaN passes through). The MISE01 boxes
  are variant 2 with complete 31-byte payloads.
- Release/destroy/delete mirroring the sibling lifecycle; `allocate_box_new`
  (0x3a8), stream kind-35 Init-link, LoadData and release arms,
  content-loading allocation and disposal arms.

## Hard limits and documented deviations

- Box physics, breaking and collision effects are unimplemented; the retained
  words await their effect owner. The f32 volume math documents the portable
  precision policy (boundary flips within 1 ulp of 0.01/1000.0 are possible
  against x87; MISE01 volumes sit far from the bounds).
- The flag-missing path consumes nothing and clamps the Init values, mirroring
  the native always-run clamp. Independent review outstanding.

## Verification

```sh
cargo fmt --all -- --check
cargo test --locked -p lhr-engine --all-features --lib -- box_
cargo test --locked -p lhr-engine --all-features --lib
cargo clippy --locked --all-targets --all-features -- -D warnings
```

Constructor/volume/clamp/flag unit tests, world tests (factory marker and
kind, volume-selected Init, all flag variants with clamps, release refcount
and deletion) and a session test (flag-2 record links against fixture bounds,
cursor advances, disposal completes). Full suite: 1838 passed, 11 ignored, 1
pre-existing ARM64-only `ph_thing_ai` failure. MISE01 probe: all 293 actors
load with no UnsupportedKind left; the actor-intake phase completes. The
remaining load blocker is the pre-existing model SkinPose path
(`QuaternionFromMatrixUnavailable`), untouched by this change. No fresh GPU
captures on macOS ARM64; Linux software-Vulkan captures stay current.
