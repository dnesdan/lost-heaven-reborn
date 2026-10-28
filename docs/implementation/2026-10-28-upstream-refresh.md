# Upstream refresh — 2026-10-28

Reference: [mafia-re `dc867c3`](https://github.com/Marvisak/mafia-re/tree/dc867c3238488de27d094bb79b6550e392566e3c),
two commits after `eb5a9216eea23210e96a3148e351e8211b27e240`.
LHR base: `1d2fd3be4e46275dc7c29f3f372b19f93952b738`.
These are fresh pinned GitHub source reads and a complete changed-file listing,
not a new full local checkout: the execution host is disconnected and the
fallback cannot resolve download hosts. Existing checkout/runtime receipts
retain their original dates and revisions.

## What changed in the reference

| Area | Newly available evidence | Remaining boundary |
| --- | --- | --- |
| Game lifecycle | `game/mission/game.cpp` now has Init, Done and DoneIntern bodies and numerous called helpers. | The file explicitly says these three bodies are not matched yet; unreached dependencies still have stubs. Treat this as source evidence, not complete binary/runtime validation. |
| Programs | `C_program.cpp` now supplies Init, Done, Compile, instruction recognition, CallSubroutine and CallProcess. | DecodeParams still returns false; Process remains a stub. This is not an executable compiler/VM. LHR already has its own bounded keyword and scheduler contracts. |
| Navigation | `pathweb.cpp` now supplies DeRiciConns, called by Game Init. | LoadNodes is still a stub upstream. LHR's existing owned loader needs a production check.bin adapter; it must not be replaced by that stub. |
| Actors | Base actor and several derived actor files expanded. | `jp_ai_entity.cpp` still has a stub constructor. Actor 27 is not solved by copying an upstream placeholder. |
| Mission/camera/UI | Mission lifecycle helpers, camera, HUD and load/save sources expanded. | C_mission::Tick remains a stub. Each new caller requires its real owners and ordered effects, not empty successful callbacks. |
| Cutscenes | Latest commit adds the internal actor-removal path and clarifies shared record ownership. | Record Reset remains a stub; reflected ownership is not permission to silently discard missing behavior. |

Pinned sources: [game lifecycle](https://github.com/Marvisak/mafia-re/blob/dc867c3238488de27d094bb79b6550e392566e3c/game/mission/game.cpp),
[programs](https://github.com/Marvisak/mafia-re/blob/dc867c3238488de27d094bb79b6550e392566e3c/game/mission/C_program.cpp),
[navigation](https://github.com/Marvisak/mafia-re/blob/dc867c3238488de27d094bb79b6550e392566e3c/game/ai/pathweb.cpp),
[mission](https://github.com/Marvisak/mafia-re/blob/dc867c3238488de27d094bb79b6550e392566e3c/game/mission/Mission.cpp),
[entity](https://github.com/Marvisak/mafia-re/blob/dc867c3238488de27d094bb79b6550e392566e3c/game/ai/jp_ai_entity.cpp),
[latest cutscene change](https://github.com/Marvisak/mafia-re/commit/dc867c3238488de27d094bb79b6550e392566e3c).

## Implemented dependency in this change

`MissionComponent::de_rici_conns` implements the newly available reset over the
existing owned navigation graph. It clears only bits 0x1000 and 0x4000, visits
active nodes and links in reverse order, and writes each node before its links.
It preserves all other bytes and spare capacity. Private borrowed storage views
contain packed-layout access and inert allocation-identity checks; no new graph,
CPU model, host pointer or unsafe block is introduced. Reached errors retain the
modified prefix and interrupt the owner. Empty loops do not validate unused
backing eagerly. `WebPathNodeView::flags` exposes a read-only named observation.

This is a dependency, not a call from a completed GameInit or a finished
pathfinding system. It does not advance the historical MISE01 actor-27 frontier.
No function-coverage or whole-system parity credit is added.

Thirteen Rust regressions were authored, not executed. They include actual
synthetic loader ownership, all u16 flag values, unused capacity, reverse order,
partial failure, liveness/domain checks and rejected replay. A local source-level
C++ oracle adapted from the reference passed all 65,536 flag values plus order,
idempotence and spare-storage checks with GCC/Clang at O0/O2. That verifies the
reference loop's expectations, not Rust code, PC layout or the original game.
Rust compilation, rustfmt, Clippy, workspace tests and two independent reviews
remain required. No renderer or new screenshot ran for this change.

## Next implementation order

1. Consolidate overlapping audio PRs #50/#51; keep one implementation and carry
   distinct regression coverage forward. Validate the selected exact revision.
2. Connect existing LoadNodes ownership to production check.bin intake in the
   original MissionOpen order (check.bin, road.bin, OpenBin). Integrate this
   reset at the evidenced GameInit call, with real content and cleanup probes.
3. Reconstruct C_entity's required constructor/Init/LoadData dependencies with
   the private REA evidence; do not substitute a generic actor for type 27.
4. Translate new Game Init/Done and mission-program lifecycle bodies into named
   operations over actual owners. Preserve ordering and partial failure;
   implement callbacks before claiming their caller complete.
5. Connect compiler parameter decoding, executable instructions and the live
   scheduler, then the original player, camera, input and HUD. Existing limited
   diagnostic source adapters must not be presented as the original compiler.
6. Prove original objectives, failure, restart and checkpoint behavior, then
   expand content/platform checks and original lighting/weather/audio parity.

Keep the [existing roadmap](parity-roadmap-main.md) for acceptance gates and
[baseline integration record](2026-10-28-pr-integration.md) for executed evidence.
This refresh supersedes its old upstream-stub assumptions, not its test results.
