# Reconstruction goals

Build a standalone Rust engine that runs the original PC campaign from the
user's own assets. The immediate acceptance target is the playable first
mission, from the original menu through completion. **All six gates remain
open.** [Coverage](../COVERAGE.md) records the unchanged accepted-function
ledger and system gates. The campaign is not playable.

| First-mission gate | Required evidence |
| --- | --- |
| Original startup | Menu starts MISE01 with its actors, resources, scripts and update order. |
| Playable control | Original player/taxi movement, camera, collision and damage respond correctly. |
| Mission progression | Objectives, pursuit, events and replay transitions reach completion. |
| Presentation needed for play | Models, animation, materials, text/HUD and required audio support play. |
| Repeatability | Restart and repeated runs match representative original-runtime and interactive sequences. |
| Standalone execution | User assets suffice; original executables and REA remain development inputs. |

## Current foundation and next work

At the 2026-11-01 audit baseline, the MISE01 diagnostic loads all 3,183 objects and 38 genuine actors,
including both Car/Vehicle owners. Original Init and LoadData complete for both
cars, preserving unknown parameter padding and actual frame-record ownership.
It stops at `UnsupportedKind(20)`, cursor 3,814,295, entry `0x544a16`.
Terminal disposal completes with all tracked lifetime counters zero. This remains
source inspection, not completed MissionOpen or authored play.

Actual global Init/Done and reset/start now use the source-owned CProgram.
PRs #58–60 add the named 13/18/43 parameter family and execute
FINDFRAME/FINDACTOR through live frame/actor lookup into the actual VM banks.
PR #64 composes those operations with the actual retained mission content,
ActorWorld and caller's live Scene/ModelArena. The loaded-session execution
interface in PR #67 now uses those same owners and one validated Scene selection.
Actual constructed Game/lifecycle integration and further command effects remain
separate work; the earlier INTRO stop is not a fresh result at the current revision.
PR #65 adds actual
TrafficGenerator source intake and teardown: the Morello probe advances through
four such actors to kind 6, with all tracked owners released after that stop.
This does not implement Traffic GameInit, Tick or spawning.
PR #66 loads the actual menu/Salieri companion animations and preserves shared
controller resources. Both original-data probes reach the mission animated-model
list dependency and then dispose all tracked owners; actual scheduling and
rendered original menu animation remain open.
Recorded intake consumes record01a; actual mission actor binding first needs
LoadDifferences(dif01a.chg), followed by the remaining recording pipeline.
[Reviewed implementation and validation](implementation/2026-10-29-car-replay-programs.md).

Lazy animated materials now consume the live driver timeline through their
actual owners. A public fixture proves changing GPU material resources and
rendered pixels. The original MISE01 material owner advances its frame index
and texture alias under controlled steps, but the unchanged-camera captures
have zero changed pixels; visible original-content animation remains unproven.
Engine Lab exposes live resources and supports controlled inspection,
pause and stepping; it does not close a gameplay gate.

Track the [nine current follow-ups](implementation/2026-11-01-parity-continuation.md)
and LS3D/library parity separately from these six first-mission acceptance
gates. Matching the pinned mafia-re reference does not establish Game.exe
mission compatibility. The older seven-stage roadmap retains its historical
source and execution scope.

1. Reconstruct actual MissionOpen and active Close in native order, including
   navigation from `check.bin`, road loading, cache and sound dependencies.
   Connect the owned navigation graph without skipping earlier unavailable work.
2. Continue reached actor factories and LoadData branches, then original program
   compilation, VM dispatch, initialization and Game scheduling over the retained
   live owners. Preserve executed prefixes, reached errors and cleanup order.
3. Connect the player, taxi, original camera, active HUD/profile, collision and
   damage. Existing controls, 64-bit key mask, camera, definition tables and
   load/save helpers are dependencies, not completed gameplay.
4. Complete play-critical model animation, controller timing/blending/events,
   rotation/scale and animated lit visuals; verify original menu animation and
   original-content material/audio behavior. General GPU fog and environment
   effects remain open.
5. Connect navigation and AI to objectives, pursuit, cutscenes, checkpoints and
   mission completion. Run repeatable menu-to-completion and restart sequences
   against suitable original-runtime evidence, then validate target platforms
   and hardware audio.

PC REA instruction receipts and PS2 symbols supply bounded evidence for names,
scalar behavior and control flow. They do not prove the full native call graph.
[Current evidence](implementation/parity-continuation.md) ·
[coverage](../COVERAGE.md) · [progress](PROGRESS.md) ·
[preceding goals, verbatim](history/2026-10-26-pre-final-documentation.md#docsgoalsmd).
