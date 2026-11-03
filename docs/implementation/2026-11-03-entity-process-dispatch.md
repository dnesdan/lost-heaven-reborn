# Entity Process owner-command dispatch (WAIT/VIDIM/SHUT_UP/NAPRDELIVAUTE/FOLLOWPLAYER) — 2026-11-03

Reference code: [mafia-re C_program.cpp](https://github.com/Marvisak/mafia-re/blob/d0fbd6dba9ac5b31cabd6661c9bb2b855eab5496/game/mission/C_program.cpp)
(SETEVENT/HUMAN_ADDWEAPON execution shapes) with C_entity::Process
(GAME 0x51fda0) bodies recovered from the shipped binary. Tested against the
local Game.exe SHA-256
08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27.

## Protocol discovery (capstone + decompile structure)

- A `break` out of the Process switch falls through to the shared
  NextCommand epilogue: `break` COMPLETES and advances. Only the explicit
  `goto` to the time-accumulation epilogue stays on the current command.
  Earlier working notes had this backwards; the corrected reading is
  consistent across VIDIM/SHUT_UP/NAPRDELIVAUTE (apply-and-advance) and WAIT
  (parks fresh: no flags → stay).
- WAIT advances only when the 0x10 flag is set without 0x20; the 0x20 timer
  branch consults a game condition with no ported source yet and stays.
- Base opcodes inside entity programs reuse the existing base dispatch arms
  (the native entity Process delegates unknown IDs to base DecodeParams, and
  base recognition maxes at ID 397, so opcode-range routing is airtight and
  base-only programs are behaviorally unchanged).

## Implemented scope

- Authoritative per-actor Process scratch (`EntityProcessState`: +0x46c
  protocol word, +0x4b4 visibility word, +0x630 shut-up byte) owned by
  `EntityState`, with world load/store and lookup-backend round-tripping
  (default-refusing trait methods; the mission world serves live state).
  Programs borrow it per command; nothing is cached across ticks.
- WAIT flag protocol (park/advance, timer branch stays documented), VIDIM
  blind bit (mirroring the float-bits quirk: only 0.0 clears, the 1 branch is
  dead by decode), SHUT_UP mute byte (inverted polarity as evidenced),
  NAPRDELIVAUTE/FOLLOWPLAYER event flags (including the native other-words→1
  fallthrough). All other 0x1000+ opcodes refuse with `UnsupportedCommand`,
  like unexecuted base commands.
- Execution without an owning actor, or against a backend without a lookup
  world, refuses explicitly (no silent default actor).

## Hard limits and documented deviations

- No tick wiring: nothing assigns program owners or steps entity actors in
  production yet, so mission scripts do not advance outside tests. Owner
  assignment and the per-tick walk arrive with Game tick wiring.
- WAIT's timer branch never completes (no game-condition source); armed waits
  park like the native wait while its condition is unmet. The 0x2000/0x200
  advance protocol has no producer yet and is deferred with it.
- Non-yes/no SHUT_UP words map to 1; the native fallthrough keeps an
  incidentally matching value there, which no shipped line exercises.
- The `MissionLookupWorld` 6-line delegation is compiler-checked and
  shape-mirrored to the adjacent tested `actor_frame`, but has no dedicated
  execution test yet; direct coverage arrives with tick wiring.
- human_talk/human_addweapon/act_setstate/dialogue/inventory/event systems
  stay open (Sam's program now runs lines 1–10 through the executed owner
  commands and refuses at the line-11 `human_addweapon`). Independent review
  outstanding.

## Verification

```sh
cargo fmt --all -- --check
cargo test --locked -p lhr-engine --all-features --lib -- entity_
cargo test --locked -p lhr-engine --all-features --lib
cargo clippy --locked --all-targets --all-features -- -D warnings
```

State default/round-trip tests, world load/store tests (including NotEntity
and stale references), dispatch tests over a state-serving fake backend
(word application, flag transitions, park/arm/refuse transitions,
ownerless refusal, past-end scheduler stop), and the reworked mixed-table
test (WAIT parks, armed WAIT advances, VIDIM decodes beside both blind
branches). Full suite: 1843 passed, 11 ignored, 1 pre-existing ARM64-only
`ph_thing_ai` failure. MISE01 load probe unchanged (no load-path change).
No fresh GPU captures attributable to this change (execution-only milestone).
