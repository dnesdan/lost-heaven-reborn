# Dialogue batch (HUMAN_TALK/DIALOG_*/SUBTITLE)

**Base:** `08398317c8dc423d` · **mafia-re:** `d0fbd6d` · ** CProgram:** `game/mission/C_program.cpp` DecodeParams `CMD_HUMAN_TALK` 1364-1378 (`int[4]` with `v[3]=-1`), `CMD_DIALOG_BEGIN/CAMSWITCH` 958-995 (`int[2]`), `CMD_DIALOG_END` 774-784 (0), `CMD_INTRO_SUBTITLE_ADD` 974-995 (`int[2]`), `CMD_SUBTITLE_ADD` 737-757 (`int[1]`); Process `CMD_HUMAN_TALK` 4826-4850, `CMD_DIALOG_BEGIN` 3862-3886, `CMD_DIALOG_CAMSWITCH` 3895-3913, `CMD_DIALOG_END` 3887-3894, `CMD_INTRO_SUBTITLE_ADD` 6041-6046, `CMD_SUBTITLE_ADD` 7561-7563; `g_dabing.*` `g_camera.*` `g_indicators.*`.

## Summary

- **Decode:** `HUMAN_TALK` (132) requires 2–3 words → `int[4]` with `v[0]=atoi(p0)` actor reg, `v[1]=atoi(p1)` speech id, `v[2]=atoi(p2)` kind or `0`, `v[3]=-1` persistent channel. `DIALOG_BEGIN` (78) and `DIALOG_CAMSWITCH` (80) are already `int[2]` via pair family; `DIALOG_END` (79) is 0-param; `INTRO_SUBTITLE_ADD` (239) `int[2]`; `SUBTITLE_ADD` (240) `int[1]` via scalar. All use dedicated `HumanTalkParameter` for 132 with `Allocate/Convert/Release` effects and `HostRuntimeDiscard` `human_talk_allocations`; other dialogue opcodes reuse existing families.

- **Execution:** All six dialogue opcodes are admitted as decode and advance as `Advance::Next` (no-op) for now; `HUMAN_TALK` channel persistence (`v[3]`) is preserved in the instruction's `HumanTalkParameter` but not yet consumed, `DIALOG_*` camera/dabing side-effects (`DialogStart/Look/Stop`, `Dabing*`, `AddHumanTalk`) remain stubs. This matches the current `mafia-re` stub status for `G_Camera::Dialog*` (`g_camera.cpp:1314-1338` `StubLog`) and `Dabing*` (`g_dabing.cpp:46-111`).

- **Runtime:** `RuntimeFields` unchanged (no new global), `RuntimeEffect` adds `AllocateHumanTalk/ConvertHumanTalk/ReleaseHumanTalk`, `InstructionParameter` adds `HumanTalk`, `Compiler` decodes 132, `HostRuntimeDiscard` counts `human_talk_allocations`, `CProgram` handles `HumanTalk` via `HumanTalkParameterSlot`, `Process` dispatches 132/78/79/80/239/240 as `Next`.

## Tests

- **Decode:** `human_talk_parameters` unit tests for 2/3 words, channel `-1`, `int[4]` ordering, unsupported opcode, and prefix retention.

- **Dispatch:** `dialogue_commands_advance_as_noop_and_count_in_discard` (4 opcodes advance, `human_talk_allocations` counted).

- **Full suite:** `1866 passed / 11 ignored / 1 pre-existing ARM64` (was `1863`). Clippy + fmt clean, `progress.py --check` passes, MISE01 capture unchanged (decode/execution-only).

## Open

- `HUMAN_TALK` blocking poll (`DabingIsSpeaking`/`AddHumanTalk` channel) and `DIALOG_*` camera/HUD (`INDF_MAP`/`CROSSHAIR`, `SetIdleAnims`) remain stubbed; `INTRO_SUBTITLE_ADD`/`SUBTITLE_ADD` `mafiaIndicators` vectors remain stubbed.
