# Entity program init transport (compile pass over embedded programs) — 2026-11-03

Reference code: [mafia-re C_program.cpp](https://github.com/Marvisak/mafia-re/blob/d0fbd6dba9ac5b31cabd6661c9bb2b855eab5496/game/mission/C_program.cpp)
(Compile entry and program Init ownership) with the global program init
already ported in `mission_program_lifecycle.rs`. Tested against the local
Game.exe SHA-256
08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27
(compile behavior only; no new binary addresses).

## Implemented scope

- `MissionProgramWorldContext::init_entity_programs`: compile every Ready
  entity actor's embedded program that has source but no runtime yet, in
  ownership order, through the context's effects backend. Each program is
  taken out of its actor, initialized, and returned on every path including
  failed inits. Already-initialized programs are left untouched and
  uncounted; program-less, non-Ready and non-entity actors are skipped.
- Like the global program init, the caller must run under a
  Pc24PrecisionState scope so transcendental boundary admissions can record
  responsibility; the method itself takes no scope (it borrows only the
  context backend, mirroring `tick_entity_programs`).
- A stray tick-protocol doc comment left attached to
  `process_program_in_world` by an earlier edit is corrected to describe
  that method.

## Hard limits and documented deviations

- Nothing calls this in production yet: mission scripts do not compile or
  advance outside tests. The production caller (mission startup / Game tick
  wiring) arrives with the run loop.
- Init failure stops the pass at the first failing actor with its identity;
  no partial-retry semantics is invented.
- Real-data program sources are covered at decode level by the earlier
  79-mission probe (9708 entity-table hits, 285 base fallbacks, one
  native-identical rejection); compiling shipped sources inside unit tests
  is out of scope (synthetic fixtures only). Independent review outstanding.

## Verification

```sh
cargo fmt --all -- --check
cargo test --locked -p lhr-engine --all-features --lib -- tick_entity
cargo test --locked -p lhr-engine --all-features --lib
cargo clippy --locked --all-targets --all-features -- -D warnings
```

Extended full-context integration test (real world, owned sourced program:
init pass compiles exactly once, second pass is a no-op, ticks then advance
and park as before). Full suite: 1844 passed, 11 ignored, 1 pre-existing
ARM64-only `ph_thing_ai` failure. MISE01 load probe and capture unchanged
(bit-identical rendering; lifecycle-only change).
