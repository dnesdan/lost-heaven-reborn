# Reconstruction goals

Build a standalone Rust engine that runs the original PC campaign from the user's
own assets. The immediate goal is the playable first mission from the original
menu through completion. Static inspection and standalone driving do not satisfy it.

## First-mission acceptance

All six gates remain open. [Coverage](../COVERAGE.md) records the separate strict
12-system registry; accepted scoped work remains 311 entries / 114 contracts.

| Gate | Required evidence |
| --- | --- |
| Original startup | Menu starts MISE01 with original actors, resources, scripts and update order. |
| Playable control | Original player/taxi movement, camera, collision and damage respond correctly. |
| Mission progression | Original objectives, pursuit, events and replay transitions reach completion. |
| Presentation needed for play | Models, animation, materials, text/HUD and required audio support play. |
| Repeatability | Restart/repeated runs match representative original-runtime and interactive sequences. |
| Standalone execution | User assets suffice; original executables and REA remain development-only inputs. |

Linux software Vulkan is exercised. Native macOS/Metal remains untested; the Linux
Apple cross-check is blocked by missing CoreAudio SDK headers.

## Remaining dependency order

1. Connect saved-lightmap loading to the actual scene `LMAP` payload and visual
   replacement. Unconstructed Lit drawing does not supply saved lighting.
   Complete SceneOpen progress presentation and genuine MissionOpen owners.
2. Connect original startup/menu, player/camera, graphics, input, audio,
   profiles and tables. Extend source lighting, fog, specialized visual passes
   and portal/occluder visibility through the retained scene.
3. Complete authored actor, animation, collision and vehicle lifetimes through
   real mission owners, including remaining GameInit/GameDone dependencies.
4. Complete script expressions, parameter/compiler/lifecycle cases, scheduler,
   world effects, replay, objectives and mission completion. The authored MISE01
   probe remains FINDACTOR/UnsupportedWorldBackend at PC 3 / source line 4.
5. Validate the complete first mission against original-runtime and interactive
   Rust sequences, including restart, cleanup and required presentation. Keep
   CPU/source projections, GPU checks and complete application parity distinct.

## Reconstruction rules

Use REA/Ghidra contracts keyed by binary hash and canonical entry. Actual PS2
symbols establish names; PC layout, ABI and behavior need separate evidence.
Implement dependencies through real owners, compare admitted domains, and keep
unsupported paths explicit. Two sequential independent reviews precede acceptance.
Original models/missions are general regression inputs, not implementation special
cases. Static pseudocode, scoped ports and complete systems remain separate counts.

[Current progress](PROGRESS.md) · [technical reference audit](REFERENCE_PROJECTS.md) ·
[previous detailed goals](history/2026-10-25-goals.md).
Optional [enhanced assets](ASSET_ENHANCEMENT.md) follow original-data compatibility
and the playable-mission goal.
