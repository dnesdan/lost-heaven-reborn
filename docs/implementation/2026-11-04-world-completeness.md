# World completeness probe (2026-11-04)

Verifies that every MissionOpen world input for the first mission exists and
parses, without promoting diagnostics into live owners. No gameplay gate
closes.

## Reference

mafia-re `d0fbd6d`, game/mission/Mission.cpp, `C_mission::Open` (PC 0x5409d0):
`check.bin` before `road.bin` before `OpenBin` (scene2), `cache.bin` probed
before `OpenBin` and opened after. PR3 (`pr3@b0c02ef`) is already incorporated
(see `2026-11-03-pr3-vehicle-parity.md`).

## What is added here

- `crates/lhr-engine/src/mission_world_completeness.rs` (new): mission-name
  validation (1..=64 ASCII alnum/`_`/`-`), `check`/`road`/`cache` path
  resolution in MissionOpen layout, optional-file presence + byte length via
  `Installation::read_optional` (absence is the `ROpen`-failed branch;
  resolver errors stay distinct), bounded `cache.bin` summary through the
  existing `parse_city_cache` (parts/objects/unknown chunks), and
  player/taxi association from already-parsed scene2 metadata
  (`TommyTAXIDRIVER` kind 2, `taxi` kind 4; exactly one actor + one object
  each, otherwise reported, never defaulted).
- `crates/lhr-engine/src/mission_load_probe.rs`: reports the three world
  files, the city-cache summary, and the actor association (with world
  positions and scene2 metadata counts) from a diagnostics-only second
  installation handle. Live session owners are untouched.
- `crates/lhr-engine/src/lib.rs`: registers the module and re-exports it
  under `game`.

Deliberately not done: navigation-graph loading (`check.bin` through the
`WebPath` component), road-network loading (`road.bin`; mafia-re itself is
still a stub), `base_cache.Open` for `cache.bin`, `GameInit`/scheduling, and
player/taxi control coupling. Those are the ordered follow-ups below.

## Evidence (real installation, 2026-11-04)

`--root .../mafia-pc/hra --mission mise01 --mission-load-probe
--probe-model-dir Models`, Game.exe SHA-256
`08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27`:

- `missions/mise01/check.bin` present, 953,798 bytes.
- `missions/mise01/road.bin` present, 81,032 bytes.
- `missions/mise01/cache.bin` present, 852,638 bytes.
- City cache: 352 parts, 10,546 objects, 0 unknown chunks.
- Mission actors: player found, taxi found;
  player `[238.61, -0.96, 181.94]`, taxi `[237.69, -0.81, 185.88]`
  (adjacent spawn, as authored); scene2 metadata 3,183 objects / 293 actors
  / 39 scripts / 2,817 unknown (unexecuted payload) chunks.
- Scene2 intake still consumes the full 3,840,614 bytes with zero tracked
  owners after disposal (unchanged).

## Validation

```sh
source scripts/common.sh
cargo test --locked -p lhr-engine --lib mission_world_completeness
cargo fmt --check
cargo clippy --locked -p lhr-engine --lib --all-features -- -D warnings
cargo test --locked -p lhr-engine --lib
```

Result: 3/3 new tests pass; full lib 1888 passed / 1 pre-existing
`ph_thing_ai` ARM64-only friction failure / 11 ignored (baseline had 1885
passed with the same single failure). No new ledger credit; all six
first-mission gates remain open.

## Ordered follow-ups toward playable mission 1

1. Load `check.bin` through the owned `WebPath` component + allocator in
   MissionOpen order (loader exists in `mission_web_path.rs`; session
   ownership does not). Report node/link counts against the 953,798 bytes.
2. Recover `road.bin` `C_roads::LoadData` from PC instructions via REA
   (mafia-re is a stub here); wire behind a typed road-network owner.
3. `base_cache.Open` for `cache.bin` (852,638 bytes, 352 parts) behind the
   mission cache owner; reuse the bounded parser, not the driving/inspection
   copies.
4. Minimal `C_game::Init` driver in native order (police/traffic GameInit,
   `GlobalProgramGameInit`, `GameInitStart`, indicators/effects) over the
   retained session owners.
5. Player/taxi coupling: spawn at the verified positions, original camera,
   collision + damage, keyboard controls into `vehicle_controls`.
