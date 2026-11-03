# Inventory-insert and act-state batch (human_addweapon, act_setstate) — 2026-11-03

Reference code: [mafia-re C_program.cpp](https://github.com/Marvisak/mafia-re/blob/d0fbd6dba9ac5b31cabd6661c9bb2b855eab5496/game/mission/C_program.cpp)
(DecodeParams CMD_TIMERON..CMD_QUAT_SLERP four-atoi group and CMD_ACT_SETSTATE
keyword mapping; execution bodies CMD_HUMAN_ADDWEAPON/Cmd_ACT_SETSTATE) with
C_actor.h ACTSTATE_*/ACT_STATE_* codes and the GAME 0x632800/0x6327f4
ACTIVE/INACTIVE strings. Tested against the local Game.exe SHA-256
08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27
(decode behavior only; execution mirrors upstream bodies).

## Implemented scope

- `program_quad_parameters`: four-integer DecodeParams family for opcodes
  50/59/167/294 (TIMERON/HUMAN_ADDWEAPON/RACING_MISSION6_INIT/QUAT_SLERP),
  following the pair-family pattern (backend allocation/conversion effects,
  slot, `InstructionParameter::Quad`, disposal). Decode-only for 50/167/294.
- `program_act_state_parameters`: opcode 26 decode (register plus
  ACTIVE/INACTIVE/other keyword mapping to ACT_STATE_ACTIVE/INACTIVE/NONE,
  case-insensitive) with its own slot and `InstructionParameter::ActState`.
- Execution: CMD_HUMAN_ADDWEAPON resolves the register's actor (no owner
  fallback), requires ENTITY/PLAYER humanity, and pushes the weapon record;
  CMD_ACT_SETSTATE resolves owner-or-register (an in-range empty slot
  resolves to no actor, never back to the owner) and stores the mapped
  ACTSTATE_2/0/1 word. Unresolvable actors advance without effect; mistyped
  humans are silent no-ops at the mission boundary; stale and defective hosts
  propagate.
- `BaseActorState.state_1c` is the +0x1c act-state word (C_actor.h identity;
  real seeds carry the native Init zero). The game-side change notification
  has no Game owner yet and is not recorded.

## Hard limits and documented deviations

- Weapon records store the evidenced upstream field order with an
  evidenced shape: ushort id plus pad at +0x0 (the id arrives pre-truncated
  by the `(ushort)` conversion at the command site), ints at +0x4/+0x8, null
  pointer at +0xc from the memset-0 constructor. Slot-placement and capacity
  rules, ChangeWeaponModel and WeaponChoose stay open. No shipped MISE01 line
  reads these records back (verified by census), so the target mission is
  observably exact.
- TIMERON/RACING_MISSION6_INIT/QUAT_SLERP decode but refuse execution like
  other unexecuted base commands. Dialogue/inventory-depth/event systems stay
  open (Sam's lines 1–14 now resolve to implemented commands; line-26
  `human_talk` is next). Independent review outstanding.

## Verification

```sh
cargo fmt --all -- --check
cargo test --locked -p lhr-engine --all-features --lib -- quad_ act_state act_setstate
cargo test --locked -p lhr-engine --all-features --lib
cargo clippy --locked --all-targets --all-features -- -D warnings
```

Family decode tests (four-integer group admission, keyword mapping, failure
prefix retention), dispatch tests over a state-serving fake backend (owner,
register and in-range-empty resolution, ACTSTATE mapping, u16 truncation,
silent advance, injected failure propagation, quad refusal and disposal
counts), world tests (act-state write, weapon push content and count,
NotHuman/stale refusal), and a live-world delegation test (type-mismatch
silence, stale propagation, entity-state round-trip refusal on detectors).
Full suite: 1850 passed, 11 ignored, 1
pre-existing ARM64-only `ph_thing_ai` failure. MISE01 load probe and capture
unchanged (bit-identical rendering; decode/execution-only change).
