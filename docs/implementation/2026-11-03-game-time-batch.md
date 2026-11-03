# Game time and tick batch (WAIT/GETTICKTIME/GETGAMETIME)

**Base:** `08398317c8dc423d` · **mafia-re:** `d0fbd6d` · ** CProgram:** `game/mission/C_program.cpp` `Process` `command_time` 0x24/`event_command_time` 0x28/`event_time` 0x2c/`game_time` `game.h:396` `0x2b0c`, `C_game::Tick` `game.cpp:1937` `game_time+=time`, `GetGameTime` `game.h:812`, `WAIT` `S_operand` `C_program.h:476`, `GETTICKTIME` 209/`GETGAMETIME` 210 `C_program.cpp:736` `int[1]` and `5623` (`float_regs[reg]=(float)time` vs `GetGameTime()`).

## Summary

- **Runtime:** `RuntimeFields` adds `game_time: u32` (zero-initialized, `wrapping_add(delta)` each `dispatch` alongside `command_time`/`elapsed`). `RuntimeView` exposes it, `Process` increments it, `CProgram::try_set_event` snapshots `event_time = game_time` (was `0`), and `RETURN` (3) with `GOSUB_EVENT_TIME` restores `command_time = event_command_time + (game_time - event_time)` when `!LeaveCompactBlock(false)` (previously unconditional `event_command_time`).

- **Execution:** `GETTICKTIME` (209) writes `float_regs[reg] = delta as f32`, `GETGAMETIME` (210) writes `float_regs[reg] = game_time as f32` (both `value_at` scalar `int` reg, out-of-range/bank-missing → no-op). `WAIT` (4) already `Stay` while `command_time < wait_word` (`+0.5` trunc) and now correctly accounts for event duration via the restored `command_time`.

## Tests

- **Dispatch:** `get_tick_and_game_time_write_float_regs` (dim `float_regs`, `getticktime` writes `delta`, `getgametime` writes `game_time`, `game_time` increments, `try_set_event`/`return` deltas now `event_command_time + (game_time - event_time)`).

- **Full suite:** `1869 passed / 11 ignored / 1 pre-existing ARM64` (was `1868`). Clippy + fmt clean, `progress.py --check` passes, MISE01 capture unchanged (decode/execution-only, `game_time` not yet driven by a real `C_game::Tick`).

## Open

- `C_game::Tick` global loop and `C_game::Init` `0x5a0810` remain stubs (only `mission_game.rs` constructor); `SETTIMEOUTEVENT` etc still `Remaining` no-op (no `timeout_program/event` storage yet).
