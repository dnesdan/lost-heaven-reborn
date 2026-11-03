# VM control flow and scalar execution (opcodes 0–4, 6–10) — 2026-11-03

Reference code: [mafia-re C_program::Process](https://github.com/Marvisak/mafia-re/blob/d0fbd6dba9ac5b31cabd6661c9bb2b855eab5496/game/mission/C_program.cpp#L2868-L2870)
with DecodeParams/DecodeOperand/DecodeLet/DecodeIf and C_program.h layouts;
`rnd` is C_actor::rnd (MSVC rand); LET division guard is MRG_ZERO 1e-8
(ls3df.h). Tested against the local Game.exe SHA-256
08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27.

## Implemented scope

- Decode for LABEL/GOTO/GOSUB/WAIT/LET/IF through a new owned `FlowParameter`
  payload (`program_flow_parameters.rs`): label copies, register-or-literal
  operands, LET/IF operator scans in original priority order, length gates
  (16/15), and the native-false mapping (count/shape rejections drop the
  pending instruction with a compile error). The six selectors are disjoint
  from all existing arms by test.
- Execution in `Process::dispatch`: float-bank DIM_FLT (mirrors the DIM
  bank arms with the same 65,536 admission cap), LET/IF over live registers,
  GOTO/GOSUB/RETURN with reverse case-insensitive label search and the
  indirect `flt[N]` form, WAIT with the accumulating command clock, END as a
  staying no-op under the scheduler's pre-cleared continuation, and RND over
  an owned MSVC LCG sequence. Every taken path resets the command clock
  (NextCommand); only WAIT preserves it.
- Release order float → frame → actor on Done, matching the native order;
  discard accounts flow payloads and float slots.

## Hard limits and documented deviations

- No Game/actor/scene callees: FRM_SETON and all other effect commands still
  stop at `UnsupportedCommand`. END does not run ClearAfterEnd (no saved-code
  chain or actor owners exist yet); GOSUB_EVENT returns are refused.
- Negative float-register indexes are fail-closed domain errors; the original
  reads out of bounds there. Out-of-domain float→int casts saturate where the
  original cast is undefined. LABEL names cannot overflow (stored exact).
- RND owns its LCG state from seed 1 deterministically; the original reseeds
  from the host clock at GameInit, which is not implemented. Reseed with the
  real GameInit before claiming original sequences.
- Host-precision family (ARM64 macOS): `ph_thing_ai::move_box_floor_contacts`
  fails in the portable friction path with
  `NumericDomain("physical retained friction dependency")` where the x86_64
  x87 backend accepts. Same class as the two documented x87 high-FIP
  prerequisite failures; the files involved are untouched by this change
  (last touched in PR #36) and share no code with the VM. Do not weaken the
  test; compare the portable trig domain against x87 before changing defaults.
- No accepted-function credit is claimed by this change; gameplay, GameInit
  and mission scheduling remain open. Independent review outstanding (no
  delegated reviewers available).

## Verification

```sh
cargo fmt --all -- --check
cargo test --locked -p lhr-engine --all-features flow
cargo test --locked -p lhr-engine --all-features program_runtime
cargo test --locked -p lhr-engine --all-features
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
```

Nine flow-decode tests (operand shapes, label/goto/wait/let/if admission,
operator-priority quirks, length gates, interruption retention, null
allocation, arm disjointness across all 398 selectors), one CRT atof-prefix
test with bit-exact vectors, and fifteen execution tests (DIM_FLT, all LET
operators, MRG_ZERO boundary, LET/IF guards, all IF branches, GOTO forward/
backward/missing/indirect/last-wins, gosub nesting, stop flag, WAIT hold/
release/clock-reset, bit-exact RND vectors, END idempotence, Done ordering)
plus two rewritten refusal tests. Bit-exact RND vectors (seed 1: draws 41,
18467, 6334), LET operator-priority quirks, MRG_ZERO boundary, WAIT clock
hold/release/reset, gosub nesting and Done ordering are covered by synthetic
tests. Original-data probes must show an unchanged MISE01 load frontier
(kind 20) after this change.
