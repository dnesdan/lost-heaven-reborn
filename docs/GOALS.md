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

The MISE01 probe loads 3,183 scene objects, 293 actors, 39 scripts, the
13,857-node navigation graph and the 229-node road network, and disposes
all tracked owners to zero. This remains source inspection, not completed
MissionOpen or authored play. Global programs, VM dispatch, player/taxi
coupling, GameInit and mission scheduling remain separate work; recorded
playback still needs its difference pipeline.

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
[coverage](../COVERAGE.md) · [progress](PROGRESS.md).
