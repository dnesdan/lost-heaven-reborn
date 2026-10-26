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

The current MISE01 diagnostic loaded all 3,183 objects and stopped at the
ordered Game actor-dispatch boundary (cursor 3,794,014, actor `0x544a16`).
The same owner produced 267 draws, including five real sky passes, then closed
cleanly. This is real model and scene intake, but not completed SceneOpen,
MissionOpen or authored mission play. The explicit `--probe-model-dir Models`
CPU probe independently reaches that same actor boundary and releases all
tracked resources, including 423 textures, through terminal disposal.

1. Connect actor registration, deferred sound links, mission scripts and
   scheduling in source order; preserve the reached errors and live owners.
2. Connect the player, taxi, active HUD/profile, collision and mission
   objectives. Current Game controls, 64-bit key mask, camera, four tables and
   load/save helpers are dependencies, not completed gameplay.
3. Complete play-critical animation and presentation: `menugl` animation in
   `missions/00menu`, 5DS rotation/scale, texture-animation ticks, animated lit
   visuals, general GPU fog and environment effects.
4. Run repeatable authored first-mission sequences and compare the necessary
   visual, audio and gameplay behavior with suitable original-runtime evidence.

PC REA instruction receipts and PS2 symbols supply bounded evidence for names,
scalar behavior and control flow. They do not prove the full native call graph.
[Current evidence](implementation/mafia-re-remediation.md) ·
[coverage](../COVERAGE.md) · [progress](PROGRESS.md) ·
[preceding goals, verbatim](history/2026-10-26-pre-final-documentation.md#docsgoalsmd).
