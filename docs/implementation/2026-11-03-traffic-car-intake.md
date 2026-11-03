# Traffic-car actor intake (kind 12) — 2026-11-03

Reference code: [mafia-re C_trf_c.h](https://github.com/Marvisak/mafia-re/blob/d0fbd6dba9ac5b31cabd6661c9bb2b855eab5496/game/vehicles/C_trf_c.h)
with C_mission::CreateActor and Mission.cpp actor arms. Tested against the
local Game.exe SHA-256
08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27
(constructor VA 0x4aa7a0, Init 0x4aa860, LoadData 0x4aad20,
vtable VA 0x624c98, 0x26c bytes; LoadData slot 20 and Init slot 18 confirmed
against the detector/bridge/door vtable layout).

## Implemented scope

- Owned `TrafficCarState`/`TrafficCarSeed` (`traffic_car_actor.rs`): the exact
  constructor body (zeroed words, float words 180.0/150.0/170.0 at
  +0x224/+0x228/+0x22c). The three unit floats go to shared world tuning
  storage, matching the native static writes.
- `refresh_traffic_tuning`: the Init global word copy, sequenced before the
  base init in intake so it persists even when the base init refuses the
  frame, exactly like the native order. The base init itself (null → false)
  rides the generic intake init.
- `load_traffic_car_data`: versions 4 and 5 share the four-word header plus
  the counted 0x20-byte element array, retained opaquely (NUL-terminated name
  plus flag words per element); other versions consume only the version word.
  Short whole elements are skipped unconsumed by the native guard, mirrored
  here. Oversized counts fail closed through bounded host reservation instead
  of the native unchecked allocation. The MISE01 car (`doprava_auta`) is
  version 5 with a complete 280-byte payload (4 + 16 + 4 + 8×0x20).
- Release/destroy/delete mirroring the bridge/door lifecycle; `allocate_traffic_car`
  (0x26c), stream kind-12 Init-refresh, LoadData and release arms,
  content-loading allocation and disposal arms.

## Hard limits and documented deviations

- Traffic driving, node following and car-link effects are unimplemented; the
  retained header words and element bytes await their effect owner.
- The version-word-missing path consumes nothing and succeeds, as for the
  door. Independent review outstanding.

## Verification

```sh
cargo fmt --all -- --check
cargo test --locked -p lhr-engine --all-features --lib -- traffic_car_tests
cargo test --locked -p lhr-engine --all-features --lib
cargo clippy --locked --all-targets --all-features -- -D warnings
```

Constructor/tuning/version unit tests, world tests (factory bytes and kind,
version-5 load with synthetic header and named elements, unknown/empty/
truncated inputs, release refcount and deletion) and a session test
(single-element version-5 record loads, cursor advances, disposal completes).
Full suite: 1823 passed, 11 ignored, 1 pre-existing ARM64-only `ph_thing_ai`
failure. MISE01 probe: no kind-12 error; frontier unchanged at kind 25
(pumper precedes the traffic car in file order). No fresh GPU captures on
macOS ARM64; Linux software-Vulkan captures stay current.
