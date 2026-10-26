# Reconstruction goals

Build a standalone Rust engine that runs the original PC campaign from the user's
own assets. The immediate goal is the playable first mission, from the original
menu through completion. Inspection and standalone driving do not satisfy it.

## First-mission acceptance

All six gates remain open. Accepted scoped work stays **311 entries / 114
contracts**, strict **0/12**, whole-function coverage **N/A**.

| Gate | Required evidence |
| --- | --- |
| Original startup | Menu starts MISE01 with original actors, resources, scripts and update order. |
| Playable control | Original player/taxi movement, camera, collision and damage respond correctly. |
| Mission progression | Original objectives, pursuit, events and replay transitions reach completion. |
| Presentation needed for play | Models, animation, materials, text/HUD and required audio support play. |
| Repeatability | Restart/repeated runs match representative original-runtime and interactive sequences. |
| Standalone execution | User assets suffice; original executables and REA remain development-only inputs. |

## Current foundation and next dependencies

Genuine visual replacement and saved lighting now feed actual scene-owned
resources, UV2/atlases, the active Camera's three draw passes and bounded
portal/occluder visibility. Admitted nearby cache selections create actual shared
model instances; guarded owners retain interrupted failures. This does not
complete original startup, streaming or mission execution.

1. Continue the real scene2 consumer from **44 records / cursor 18693**:
   implement the reached `FRAME_DUMMY` (6) factory for `detector_konec`, then
   subsequent genuine dependencies. Complete SceneOpen progress callbacks and
   production MissionOpen owners without skipping reached failures.
2. Diagnose the dark or absent road and sky; the cause remains unresolved.
   Extend ordinary lighting/material branches, fog, specialized visual passes,
   remaining camera/driver cache effects and saved-lightmap variants. Preserve
   actual resource ownership and distinguish the host cache selector from the
   original city-streaming controller.
3. Connect original startup/menu, player, input, audio, profiles and tables;
   complete actor, animation, collision and vehicle lifetimes through mission
   owners and original update scheduling.
4. Complete script expressions, compiler/parameters, world effects, replay,
   objectives and mission completion. The separate authored MISE01 script probe
   remains FINDACTOR/UnsupportedWorldBackend at PC 3 / source line 4.
5. Validate complete first-mission runs, restart and cleanup against relevant
   original-runtime evidence and interactive Rust sequences. This milestone adds
   no original executable/function execution or parity claim.

## Reconstruction rules

Use REA/Ghidra contracts keyed by binary hash and canonical entry. Actual PS2
symbols establish names; PC layout, ABI and behavior need separate evidence.
Implement dependencies through real owners, preserve explicit unsupported
boundaries, and require two sequential independent reviews. Original models and
missions are general regression inputs. Static pseudocode, scoped ports and
complete systems remain separate counts.

Linux software Vulkan is exercised; native macOS/Metal remains untested.
[Progress](PROGRESS.md) · [coverage](../COVERAGE.md) ·
[reference audit](REFERENCE_PROJECTS.md) ·
[complete predecessor goals](history/2026-10-26-scene-render-predecessor.md#goals).
Optional [enhanced assets](ASSET_ENHANCEMENT.md) follow original-data compatibility
and the playable-mission goal.
