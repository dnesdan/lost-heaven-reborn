# PR3 vehicle parity (2026-11-03)

Incorporates mafia-re follow-up PR #3 (`hellokittyfan48:game-vehicle-functions-2`,
commits `32b1b33..b0c02ef`, 5 commits, 615+/277-) on top of pinned
`d0fbd6d`. Only byte-exact, mission-relevant bodies are ported through typed
owners; stubs stay explicit boundaries. No gameplay gate closes.

## Pinned reference

- Base: `mafia-re@d0fbd6d` (after PR #2 merge `a1e7110`).
- PR3 branch: `pr3@b0c02ef` (`More vehicles`, `More vehicle functions`,
  `More car functions`, `SaveData and LoadData functions`, `pico :D`).
- Game.exe SHA-256 `08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27`;
  PS2 `SLUS_206.71` SHA-256 `adf4944c5cdac23fcc4e9af9ef3ec1ba6234497e57cec2af2ff1a7a15c62a46f`
  corroborates class names only.

## What PR3 adds upstream

| Commit | Functions (all byte-exact vs retail Game.exe) |
| --- | --- |
| `32b1b33` | `C_traffic_car` ctor/dtor/`DecrementCount`/`SaveGameSave`/`SaveGameLoad`; `C_rail_generator` ctor; `C_airplane`/`C_wagon` dtors; `C_turnout::SaveGameSave`; `C_car_ex::Init` |
| `185728f` | `C_turnout::DuplicateFrom`; `C_airplane::ShowDamageIndicator`; `G_IndicatorsClass::RefullingIndicatorSetValue`; `C_car_ex` ctor/dtor |
| `25454ec` | `C_car::GetAIPos` 0x429510; `C_car::GetNavigationPoints` 0x4299c0 |
| `ba71d6e` | `C_chunk::Write`; `SaveData`/`LoadData` (bool return) for rail-generator, railway, airplane, turnout, traffic-car, car-ex |
| `b0c02ef` | `S_shvestky::RemoveAngryActor` 0x49f780; `C_shvestky::RemoveAngryActor` 0x49f540; `ReleaseWantedHuman` 0x49f850; renames `RemoveAllCars`/`RemoveCar`/`DecrementCount`/`SendPoliceBackup`/`SendPoliceToHuman` |

Header changes split padding into named members without changing class
sizes. `docs/game-files.md` is the upstream file map; it is research input,
not a Rust contract.

## What is ported here

- `crates/lhr-engine/src/traffic_car_actor.rs`: `TrafficCarCount::decrement`
  (0x4b9560, clamp at zero), save-game tag 1 encode/decode
  (0x4bc160/0x4bc190), `TrafficSaveData` version-5 round-trip
  (0x4aabf0: three tuning words + `0x21c` + counted `0x20` names),
  `elems/mafian/police` coefficient accessors (0x63d7fc/0x63d800/0x63d804),
  explicit `remove_all_cars` (0x4bad80) / `remove_car` (0x4bbe90)
  unsupported boundaries. Vulgar working names are retired.
- `crates/lhr-engine/src/car_ai_position.rs` (new): `get_ai_pos`
  (0x429510, flag-selected row-vector transform with the original Z,Y,X /
  Z,X,Y association) and `navigation_points` (0x4299c0, four footprint
  corners from inflate + `pivot_pos_2` + `vehicle_size_2` + model Y).
- `crates/lhr-engine/src/police_state.rs`: `remove_angry_actor_from_record`
  (0x49f780, swap-with-back + pop), `remove_angry_actor_everywhere`
  (0x49f540, reverse record order), `PolicePursuitRecord` +
  `release_wanted_human` (0x49f850, full field clear + conditional
  `offenses++`), explicit `send_police_backup` (0x49eee0) /
  `send_police_to_human` (0x49f980) unsupported boundaries.
- `crates/lhr-engine/src/railway_actor.rs`: `save_payload` (0x486920:
  zero byte + six words) round-tripping the existing `load_payload`
  (0x486a20).
- `crates/lhr-engine/src/lib.rs`: registers `car_ai_position`, re-exports
  `car_ai_position`/`police_state`/`traffic_car_actor` under `game`.

## What is deliberately not ported

- `C_airplane::ShowDamageIndicator` needs the HUD `INDF_REFULLING` owner,
  which has no Rust owner yet; the formula
  `(e1+e2)/(full*2)` + add/remove flags is recorded here for the HUD
  milestone, not implemented.
- `C_rail_generator` ctor values (70=1, 74=5000, 78=130.0, 7c=1, global 10),
  `C_turnout::DuplicateFrom`/`SaveData`, `C_wagon`/`C_car_ex` ctors/dtors
  and `C_chunk::Write` have no Rust actor owner yet (LHR models
  Base/Car/Detector/Railway/Bridge/Door/TrafficCar/Pumper/BoxNew/Human/
  Player/Entity only). They are mapped with RVAs in this doc for the next
  actor-kind milestone, not stubbed into unrelated owners.
- `C_traffic_car::LoadData` 0x4aad20, `RemoveAllCars`/`RemoveCar` bodies,
  `SendPoliceBackup`/`SendPoliceToHuman`/`FreeMyTempActor` remain explicit
  unsupported boundaries with their PC entries.

## Validation

```sh
source scripts/common.sh
cargo test --locked -p lhr-engine --lib traffic_car_actor
cargo test --locked -p lhr-engine --lib -- police_state car_ai_position railway_actor
cargo fmt --check
cargo clippy --locked -p lhr-engine --lib --all-features -- -D warnings
cargo test --locked -p lhr-engine --lib
```

Result at this revision: 9/9 traffic-car, 13/13 police+car-ai+railway,
full lib 1885 passed / 1 pre-existing `ph_thing_ai` ARM64-only friction
failure / 11 ignored — same single failure as the pre-change baseline.
No new original-function ledger credit; no first-mission gate closes.
