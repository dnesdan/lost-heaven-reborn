# Mission flow batch (ENDOFMISSION/CHANGE_MISSION/MISSION_OBJECTIVES)

**Base:** `08398317c8dc423d` · **mafia-re:** `d0fbd6d` · ** CProgram:** `game/mission/C_program.cpp` DecodeParams `CMD_ENDOFMISSION` 1241-1262 (`S_event_params`), `CMD_CHANGE_MISSION` 1406-1421 (`S_change_mission_params` with `ExtrudeString` + `atof`), `CMD_MISSION_OBJECTIVES` 733-757 (`int[1]`), `CMD_MISSION_OBJECTIVESCLEAR` 774-784 (0); Process `ENDOFMISSION` 4542-4591 (fade `FadeInOut` + `SetMissionEnd`/`SetMissionPassed`/`GameFailed` menu + `13 - Game Over.mp3`), `CHANGE_MISSION` 3565-3618 (police block `PlukovnikuMasVChladkuPlajeraFrafera`, `NewMission`/`ActiveNewMission`/`ENTRY`/`KeepMissionNumber`/`Speed` + `NewMissionActors/ROT/POS`), `MISSION_OBJECTIVES` 5171-5179 (`ObjectivesAdd/Remove/Clear` on `vector<uint> objectives` at `g_loadsave.h:260`).

## Summary

- **Decode:** `ENDOFMISSION` (37) requires 2–3 words → `S_event_params` with `value1=atoi(p0)`, `value2=atoi(p1)`, `name` optional quoted string (empty `""` when 2 words, `ExtrudeString` when 3, `>63` fails). `CHANGE_MISSION` (52) requires 3 words → `S_change_mission_params` with `name1`/`name2` quoted `ExtrudeString` (≤63) and `value=atof(p2)`. Both use dedicated `EndOfMissionParameter`/`ChangeMissionParameter` slots with `Allocate/Convert/Release` effects and `HostRuntimeDiscard` counts. `MISSION_OBJECTIVES` (164) and `MISSION_OBJECTIVESREMOVE` (165) are `int[1]` via scalar, `MISSION_OBJECTIVESCLEAR` (166) is 0-param.

- **Execution:** All five mission flow opcodes are admitted as decode and advance as `Advance::Next` for now (no fade, no `SetMissionEnd`, no `NewMission` globals, no `ObjectivesAdd` vector). This matches the current `mafia-re` stub status for `FadeInOut` (`g_indicators.cpp:1204` `StubLog`), `SetMissionEnd` (`game.h:861` inline), and `ObjectivesAdd/Remove` (`g_loadsave.cpp:1026` `StubLog`, only `Clear` is real).

- **Runtime:** `InstructionParameter` adds `EndOfMission`/`ChangeMission`, `RuntimeEffect` adds `AllocateEndOfMission/ConvertEndOfMission/ReleaseEndOfMission` and `AllocateChangeMission/ReleaseChangeMission`, `Compiler` decodes 37/52, `HostRuntimeDiscard` counts `end_of_mission_allocations`/`change_mission_allocations`, `Process` dispatches 37/52/164/165/166 as `Next`.

## Tests

- **Decode:** (implicit via `mission_parameters` unit not yet added, but dispatch test covers admission) – the `impl` is `forbid(unsafe_code)` and follows the same `extrude_params` + `c_length` + `ExtrudeString` pattern as the native.

- **Dispatch:** `mission_flow_commands_advance_as_noop_and_count_in_discard` (4 opcodes advance, `end_of_mission`/`change_mission` counts).

- **Full suite:** `1867 passed / 11 ignored / 1 pre-existing ARM64` (was `1866`). Clippy + fmt clean, `progress.py --check` passes, MISE01 capture unchanged.

## Open

- `ENDOFMISSION` fade/menu and `CHANGE_MISSION` police block/`NewMission` globals remain stubbed; `ObjectivesAdd/Remove` vector remain stubbed (only `Clear` is real).
