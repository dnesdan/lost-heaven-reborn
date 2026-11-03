# Remaining MISE01 commands batch (detector/player/car generic)

**Base:** `08398317c8dc423d` · **mafia-re:** `d0fbd6d` · ** CProgram:** `game/mission/C_program.cpp` DecodeParams groups for the remaining 18 MISE01 commands (20,27,29,36,45,57,68,101,102,103,106,122,129,135,136,181,187,234) – `S_signal_params`, `S_int_float_params`, `int[1]/[2]/[3]` etc; Process bodies for `DETECTOR_ISSIGNAL` 3359, `DETECTOR_INRANGE` 213 (detector), `HUMAN_FORCE_SETTOCAR` 4346, `HUMAN_GETPROPERTY` etc; `g_detector.h` `signal` `human_types` etc.

## Summary

- **Decode:** 18 remaining opcodes are admitted via `RemainingParameter` (generic `int[1..3]` with `atoi`, `REMAINING_OPCODE_IDS` sorted, `AllocateRemaining(count)` with `vec![None;count]`). This matches the native `int[1]`/`int[2]`/`int[3]` and `S_signal_params`/`S_int_float_params` groups for these commands, and is sufficient for MISE01's `autosavegame` (129), `compareownerwithex` (101), `detector_inrange` (29), `detector_issignal` (27), `getactorsdist` (57), `human_force_settocar` (106), `human_get/setproperty` (103/102), `iffltinrange` (20), `loaddifferences` (68), `policemanager_*` (135/136), `recloadfull` (45), `soundfade` (36), `stream_create` (181), `stream_fadevol` (187), `wingman_setindicator` (122), `zatmyse` (234).

- **Execution:** All 18 are decoded and advance as `Advance::Next` (no-op) for now; detector signal, car, player and streaming side-effects remain stubs (as per `g_detector.h` `signal` 0xdc, `human_types` 0xd8, `C_car` fields, etc). This makes MISE01 fully decodable (9708+ hits, 285 base fallbacks previously, now 0 remaining).

- **Runtime:** `InstructionParameter` adds `Remaining`, `RuntimeEffect` adds `AllocateRemaining/ConvertRemaining/ReleaseRemaining`, `Compiler` decodes 18, `HostRuntimeDiscard` counts `remaining_allocations`, `Process` dispatches 18 as `Next`.

## Tests

- **Decode:** `remaining_parameters` unit for 1-3 ints, sorted IDs, failure prefix retention.

- **Full suite:** `1868 passed / 11 ignored / 1 pre-existing ARM64` (was `1867`). Clippy + fmt clean, `progress.py --check` passes, MISE01 capture unchanged (decode/execution-only).

## Open

- Detector `signal`/`human_types`/`wait_for_hit`/`dyn_colls` and car `invisible`/`break_motor` etc remain stubbed; streaming `stream_*` remain stubbed.
