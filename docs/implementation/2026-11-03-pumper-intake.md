# Pumper actor intake (kind 25) — 2026-11-03

Reference code: [mafia-re C_pumper.h](https://github.com/Marvisak/mafia-re/blob/d0fbd6dba9ac5b31cabd6661c9bb2b855eab5496/game/actors/C_pumper.h)
with C_mission::CreateActor and Mission.cpp actor arms. Tested against the
local Game.exe SHA-256
08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27
(constructor VA 0x47c540, Init 0x47c760, LoadData 0x47c860,
vtable VA 0x624990, 0x2ac bytes; LoadData slot 20 and Init slot 18 confirmed
against the detector/bridge/door/traffic vtable layout).

## Implemented scope

- Owned `PumperState`/`PumperSeed` (`pumper_actor.rs`): the exact constructor
  body in dependency order (count/float words, six indexed slots with the
  0x78+i series overwritten at slot zero by 0x1e, marker words, three -1
  words, zeroed ranges). The two lookup words at +0xd4/+0xd8 come from an
  unreconstructed table search (GAME 0x408760) and stay opaque zeros; no
  admitted reader exists yet.
- `load_pumper_data`: one consumed-and-ignored flag byte, then the
  counter/mode/price words at +0x78/+0x7c/+0x84/+0x80 (in that order), then
  the 0x18-byte indexed-slot block over the constructor values. Each read is
  individually guarded; an empty payload consumes nothing. The MISE01 pumpers
  carry complete 41-byte payloads (1 + 16 + 24).
- Pumper Init is the null-checked base Init with no derived tail, so the
  generic intake init covers it; release/destroy/delete mirror the
  bridge/door/traffic lifecycle; `allocate_pumper` (0x2ac), stream kind-25
  LoadData and release arms, content-loading allocation and disposal arms.

## Hard limits and documented deviations

- Pump fueling/payment effects are unimplemented; the retained counters and
  slots await their effect owner.
- The flag-missing path continues into the field reads (the native guarded
  flag read leaves its local behind and proceeds); only a fully empty payload
  consumes nothing. Both mirror the native guard chain. Independent review
  outstanding.

## Verification

```sh
cargo fmt --all -- --check
cargo test --locked -p lhr-engine --all-features --lib -- pumper_
cargo test --locked -p lhr-engine --all-features --lib
cargo clippy --locked --all-targets --all-features -- -D warnings
```

Constructor/accessor unit tests, world tests (factory bytes and kind, guarded
41-byte load, empty/flag-only/truncated inputs, release refcount and deletion)
and a session test (41-byte record with MISE01-shaped values loads, cursor
advances, disposal completes). Full suite: 1829 passed, 11 ignored, 1
pre-existing ARM64-only `ph_thing_ai` failure. MISE01 probe frontier moves
from kind 25 straight to kind 35 (box_new): the traffic car and the door
behind it already load. Cursor advances 3815824 → 3826046. No fresh GPU
captures on macOS ARM64; Linux software-Vulkan captures stay current.
