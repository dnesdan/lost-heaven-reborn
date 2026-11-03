# Bulk remaining iterations (7-10) – Game tick, movement, menu (provisional)

**Base:** `08398317c8dc423d` · **mafia-re:** `d0fbd6d`

This bulk documents the remaining work for the 10-iteration plan after
iterations 1-6 (weapon/actstate, event, dialogue, mission, remaining MISE01)
are complete and pushed. No new Rust behaviour is added beyond the
already-landed decode/execution-only no-ops; the engine-wide systems below
remain open and are tracked here for the next bulk.

## Iteration 7 – WAIT/timeout

- **WAIT** 4 (`S_operand` `flt[`/`atof`, `command_time` vs `wait_word` `+0.5` trunc) is already decoded via `FlowParameter::Wait` and executed as `Advance::Stay` while `command_time < wait_word` (wrapping `u32`, `float_regs` live re-read, OOB → 0). Event entry resets `command_time` to 0, `RETURN` restores `event_command_time` (delta `GetGameTime-event_time` omitted until `Game` clock) and respects `LeaveCompactBlock(false)`.
- **SETTIMEOUTEVENT** 112 (`int[2]` actor reg + event id) and siblings `SETPLAYERFIREEVENT` 115, `SETPLAYERHORNEVENT` 117, `SETPLAYERWFALLEVENT` 302 are already decoded via `RemainingParameter` (generic `int[1..3]`) and dispatched as `Next` (no `C_game::timeout_program/event` storage yet).

## Iteration 8 – Game tick loop

- **Original:** `C_game::Tick(uint time)` `game.cpp:1856` (`game_time+=time` at `0x2b0c`, `active_actors[i]->Tick(time)`), `C_mission::Tick` `Mission.cpp:472`, `C_program::Process/CallProcess` `C_program.cpp:2733` (`command_time+=time`), `GetGameTime` `game.h:812`.
- **Rust:** `mission_game.rs` only has constructor raw bytes; `mission_program_world::tick_entity_programs` is AI transport, not `C_game::Tick`. Global `game_time` and `C_game::Init` (`game.cpp:251` `0x5a0810`) remain stubs. The `event_time` snapshot uses `0` until the `Game` tick loop provides the clock (see `docs/implementation/2026-11-03-event-system.md`).

## Iteration 9 – On-foot movement + camera

- **Original:** `C_human` `action_state` `0x40c`/`tilt` `0x238`/`crouching` `0x1e4`/`fall` `0x418`, `C_player::UpdateControls` `0x5942c0` `STUB`, `G_Camera::Tick` `g_camera.cpp:283` (`C_DIALOG` `dialogSide`, `P_BEHIND` `WantPos` `0x5f2580` with `TestSphereB`, `P_MOUSE` filtering) – all `STUB` in `mafia-re` for `UpdateControls`/`AI_walk`/`Movement`, only `Do_Crouched` `0x17f8a0` is evidenced in `player_control.rs`.
- **Rust:** `player_control::do_crouched`/`set_control_lock` etc are bounded prefixes; `human_actor`/`player_actor` are constructor-only, `game_camera_reset` only `Reset` `0x5ed040`, `game_camera_projection` only `0x204890`. No integrated `Tick`/`WantPos`/`UpdateControls` poll.

## Iteration 10 – Menu/HUD/audio + dashboard

- **Original:** `GM_GameFailed` menu in `ENDOFMISSION` `4542`, `mafiaIndicators` `Timer*`/`FadeInOut`/`IntroSubtitles`, `g_dabing` `Dabing*` `0x5f4cd0/0x5f5570/0x5f5c40` stubs, `binkw32` etc.
- **Rust:** `mafiaIndicators`/`g_dabing`/`g_camera::Dialog*` remain stubs; `progress.py` ledger stays frozen (no recount pipeline); `README` already labels dashboard honestly.

## Verification

- Build/logic/rendered/interactive are still reported separately (see `docs/RUNNING.md`); `cargo test --locked -p lhr-engine --all-features --lib` → `1868 passed / 11 ignored / 1 pre-existing ARM64` (no new engine-wide integration test yet), `progress.py --check` passes, MISE01 capture `0c862527…` unchanged (decode-only).

## Next bulk

- Implement `C_game::Tick` + `GetGameTime` global clock, wire `game_time` into `SetEvent`/`RETURN` delta, decode `{`/`}` (70/71) to drive `in_block`/`LeaveCompactBlock`, implement `C_player::UpdateControls` poll + `G_Camera::Tick`/`WantPos` integration, and menu/HUD `FadeInOut`/`IntroSubtitles` – each with two review rounds as before. Until then the above remains `N/A` for parity.
