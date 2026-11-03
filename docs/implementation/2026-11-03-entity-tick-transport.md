# Entity AI tick transport (owner assignment implied by Init, per-tick stepping) — 2026-11-03

Reference code: [mafia-re C_program.cpp](https://github.com/Marvisak/mafia-re/blob/d0fbd6dba9ac5b31cabd6661c9bb2b855eab5496/game/mission/C_program.cpp)
(SETEVENT/HUMAN_ADDWEAPON execution shapes, owner-register conventions) with
C_entity::Process bodies (GAME 0x51fda0) for the WAIT/VIDIM/SHUT_UP/
NAPRDELIVAUTE/FOLLOWPLAYER words. Tested against the local Game.exe SHA-256
08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27.

## Implemented scope

- Authoritative per-actor Process scratch on `EntityState`
  (`EntityProcessState`: +0x46c protocol word, +0x4b4 visibility word, +0x630
  shut-up byte) with world load/store and lookup-backend round-tripping.
  Programs borrow it per command through default-refusing trait methods; the
  mission world serves live state. Nothing is cached across ticks.
- `MissionProgramWorldContext::tick_entity_programs`: step every Ready entity
  actor's embedded program once, in ownership order, sharing one budget. Each
  program is taken out, run, and returned on every path including failed
  ticks; program-less, non-Ready and non-entity actors are skipped. Owners
  ride the existing Init-time `set_owner` (entity and detector programs
  already receive their actor there); ownerless programs refuse inside
  dispatch like any worldless execution. This is the AI tick transport, not a
  Game tick: no game clock, camera, input or objectives run here.
- Dispatch arms for WAIT (fresh parks, armed advances, timer branch stays
  without a game-condition source), VIDIM (float-bits quirk mirrored),
  SHUT_UP (inverted polarity as evidenced), NAPRDELIVAUTE/FOLLOWPLAYER event
  flags (native other-words fallthrough kept), base fallback for non-entity
  opcodes, explicit refusal for the rest. `break` completes-and-advances
  (corrected reading verified against Sam's line 8→9 flow); only the
  time-only epilogue stays.

## Hard limits and documented deviations

- No Game tick loop calls this in production yet: mission scripts do not
  advance outside tests. The loop (clock, camera, input, objectives) arrives
  with Game tick wiring.
- WAIT's timer branch never completes; the 0x2000/0x200 advance protocol has
  no producer and is deferred with it.
- Non-yes/no SHUT_UP words map to 1; the native fallthrough keeps an
  incidentally matching value there, which no shipped line exercises.
- Callbacks during a tick must not retire the ticking actor (no such callback
  exists); the take/process/return pattern documents the invariant.
- human_talk/human_addweapon/act_setstate/dialogue/inventory/event systems
  stay open (Sam's lines 1–10 are now executable-or-implemented commands;
  line-11 `human_addweapon` is the next refusal). The `MissionLookupWorld`
  delegation is covered end-to-end by the full-context integration test
  (real world, owned compiled program, two ticks). Independent review
  outstanding.

## Verification

```sh
cargo fmt --all -- --check
cargo test --locked -p lhr-engine --all-features --lib -- entity_
cargo test --locked -p lhr-engine --all-features --lib
cargo clippy --locked --all-targets --all-features -- -D warnings
```

State default/round-trip tests, world load/store tests (NotEntity, stale
references), dispatch tests over a state-serving fake backend (word
application, flag transitions, park/arm/refuse transitions, ownerless
refusal, past-end scheduler stop), and a full-context integration test
(real world, owned compiled program, two ticks: VIDIM applies, WAIT parks,
program returns home). Full suite: 1844 passed, 11 ignored, 1 pre-existing
ARM64-only `ph_thing_ai` failure. MISE01 load probe and capture unchanged
(bit-identical rendering; execution-only change).
