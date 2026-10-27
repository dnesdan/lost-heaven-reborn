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

The current MISE01 diagnostic loads all 3,183 objects and reaches
`UnsupportedKind(27)` during ordered Game actor intake at cursor 3,795,696,
entry `0x544a16`, retaining 27 live actors. The initial 1280 × 800 inspection
produces 267 draws; visible draw counts depend on the viewport and camera.
Actor/program source intake and deferred sound
resolution are connected, but the stopped load does not prove that later arms
or deferred resolution complete on MISE01. Retained source owners do not supply
the original compiler, full VM, mission scheduler or GameInit. This remains
scene inspection, not completed SceneOpen, MissionOpen or authored mission play.

Lazy animated materials now consume the live driver timeline through their
actual owners. A public fixture proves changing GPU material resources and
rendered pixels. The original MISE01 material owner advances its frame index
and texture alias under controlled steps, but the unchanged-camera captures
have zero changed pixels; visible original-content animation remains unproven.
Engine Lab exposes live resources and supports controlled inspection,
pause and stepping; it does not close a gameplay gate.

Track the seven work stages and LS3D/library parity separately from these six
first-mission acceptance gates. Matching the pinned mafia-re reference does not
establish Game.exe mission compatibility.

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
