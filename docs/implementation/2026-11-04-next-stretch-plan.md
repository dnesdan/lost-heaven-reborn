# Plan: next stretch after roads intake (2026-11-04)

Status at this revision: session owns navigation (13,857/67,260) and roads
(229/1,124) with zeroed teardown; `C_game::Init` 31-step order is locked as
an executable driver; `FindPath` sentinel/zone-bit rules are ported with the
search cluster open. Full lib 1905 passed / 1 pre-existing failure.
Evidence inputs: mise01 `check.bin`/`road.bin`/`cache.bin`/`scene2.bin`;
REA Ghidra 12.1.4 over Game.exe `08398317…`.

Each work package below is sized for about one day and ends with executed
commands, not a merge alone. Dependency order is mandatory; later packages
must not invent the owners of earlier ones.

## A. Complete road_web.cpp (day 1)

REA facts already retained: `LoadData` 0x559e60 format + snap rule
(`ev_4ac5d63c…`), `FindPath` 0x55c4e0 contract with callee cluster
(`ev_755c7588…`; clear 0x55cb90, zone resolve 0x55be10, emit 0x47c020 /
0x55bc10, fixups 0x55bcd0, zone search 0x55bf60, edge checks 0x55b5d0 /
0x55c440, free 0x60fcf6).

1. `SaveData` 0x559d40: Ghidra auto-analysis splits dtor 0x559d00 and
   `LoadData` 0x559e60 with no entry at 0x559d40 (file-offset 0x159e40
   maps into the gap). Resolve via `annotate-native-function` creation
   with bounds derived from the dtor tail, or Hopper cross-check. Expected
   mirror of `LoadData` (version 2 + counts + arrays); do not invent it.
2. Cluster functions in dependency order: 0x55cb90 (clear), 0x55be10
   (zone resolve), 0x55bc10/0x55bcd0 (emit/fixup), 0x55bf60 (zone search),
   0x55b5d0/0x55c440 (edge validation). Each gets a typed contract plus
   synthetic tests before the next starts.
3. `SortNodesByX` 0x55c890 and `RecompileDistances` 0x55a620 as pure passes
   over the owned `RoadNetwork`; verify idempotence and node-count
   preservation on the real 229/1124 graph.
4. `MergeData` 0x55a210 documented as a boundary until its caller is found.
5. Wire `SessionRoads` behind the `Roads` component pattern only when a
   `LoadData` effect owner exists; until then the owned `Vec`s stay the
   single authority.

Acceptance: `road.bin` round-trips through parse → sort → recompile with
byte-exact snap slots; `FindPath` resolves a two-node synthetic query and
refuses `0xFFFF`; session counts unchanged after teardown.

## B. Base-cache Open (day 2)

Target: `C_cache_base_block::Open` (mafia-re file 2, PC 0x4013f0, stub
upstream) behind the mission cache owner, reusing the bounded
`parse_city_cache` (352 parts / 10,546 objects already proven). Same REA
discipline as roads: `rea function … 0x4013f0`, format + guards first,
typed owner second, synthetic tests third, real `cache.bin` (852,638 B)
verification last. Constructor 0x401070 / destructor 0x4010a0 / `Close`
0x4011f0 / `PrepareScene` 0x4012e0 come from the same file range when
needed. This unblocks the `BaseBlocks` GameInit step and complete world
rendering from cache.

## C. GameInit owner wiring (day 3)

Plug existing owners into the 31-step `game_init_order.rs` driver in this
order: `GlobalProgramGameInit` + `GameInitStart` (runner exists),
`shvestky.GameInit` (port mafia-re `C_shvestky::GameInit` 0x499cd0
behavior through `PolicePursuitRecord` + angry lists),
traffic-generator flags (extend `TrafficTuning`/`TrafficCarCount`),
`RadarClear`/`GameInit` indicators, `Mission_Start` effects. Each step
keeps its explicit unavailable-dependency error until its owner lands;
steps without owners stay failing-boundary, never silent no-ops. Probe
gains one line per wired step; the MN mission still stops before actor
`GameInit` until package D lands.

## D. Player/taxi coupling (day 4)

Native rule already recorded (last non-cutscene `ACTOR_PLAYER` in reverse,
`RadarAddCar` per car, camera at `pos − 2·dir, y + 2` + `SetPlayer`,
energy clamp, `UpdateControls`). Implement: session resolution of
player/taxi references from the live actor world (frame-name binding, not
metadata re-parse), camera placement math with the original constants,
keyboard controls into the existing `vehicle_controls` setters. Hard
gates after that: `LoadCollision` + line/sphere tests and the mission
camera `Tick` (both need their own REA packages; do not stub them as
successes).

## E. Tick skeleton (day 5)

`C_mission::Tick` (0x5407e0) + `C_game::Tick` (0x5a51c0) ordering driver
in the `game_init_order.rs` pattern, over whatever owners A–D produced.
Menu-to-mission startup (gate 1) becomes runnable end-to-end in headless
form; playable control (gate 2) stays open until D's hard gates land.

## Standing rules

One REA function cluster at a time (Ghidra imports are heavy); evidence
bundles live outside Git with IDs cited in docs; synthetic fixtures only
in tests; real-asset verification via the probe; `scripts/test.sh` gates
every commit; no force-push; vulgar working names stay retired.
