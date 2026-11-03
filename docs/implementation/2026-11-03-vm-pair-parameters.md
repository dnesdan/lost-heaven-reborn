# Two-integer VM parameters — 2026-11-03

Reference code: [mafia-re C_program::DecodeParams](https://github.com/Marvisak/mafia-re/blob/d0fbd6dba9ac5b31cabd6661c9bb2b855eab5496/game/mission/C_program.cpp#L958-L996).

## Implemented scope

- Original `FRM_SETON` (14) and `FRM_SETON1` (15) now compile through their two-integer parameter arm in the **same source-owned `CProgram` runtime**.
- Two integer words are allocated as one owned record; the existing original tokenizer mutates the source before count validation. Conversion uses the same admitted ASCII C-locale integer-prefix implementation as other original parameter arms.
- Allocation is published before conversion, and the first converted word remains visible to the second callback. Interrupted compilation retains its pending instruction and partial parameter state rather than replaying callbacks.
- Normal Done releases the owned pair payloads in reverse instruction order. Explicit host discard accounts for a pending or active pair allocation after interruption.
- Unknown selectors and other two-integer opcodes remain unavailable rather than being accepted using invented layouts.

## Validation status and hard limits

Source-backed implementation and synthetic Rust tests added for conversion ordering, count rejection, callback interruption, pending instruction ownership, reverse Done and refusal to execute `FRM_SETON` without its actual Game effects. **No local build, test, original PC execution or graphical comparison was performed in this GitHub-only editing environment**. The change must not be counted as an accepted original-function port or as closing a first-mission gate.

The original `C_program::Process` handler uses the live frame lookup, sound-specific switching, player/chase-camera guard, frame placement bookkeeping, projector clearing, shadows and actor Tick. Neither this PR nor the isolated host/Engine Lab controls implement those effects. Next implement them over `MissionProgramWorldContext` and a constructed Game owner, with the actual frame identity and callback-mutated Scene preserved.

Targeted local verification before merge:

```sh
cargo fmt --all -- --check
cargo test --locked -p lhr-engine --all-features pair_parameters
cargo test --locked -p lhr-engine --all-features original_frame_visibility_parameters
cargo test --locked -p lhr-engine --all-features interrupted_second_pair_conversion
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
```
