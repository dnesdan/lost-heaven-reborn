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

1. Connect original startup/menu/mission owners through genuine graphics, input,
   audio, profiles and tables. Current archive setup and bounded lifecycle callers
   stop at explicit provider/resource boundaries; active mission Close remains open.
2. Complete the next genuine loader dependencies: kind-12 occluder creation,
   occluder lights/build/link and original model-cache preparation. Preserve
   the camera/top-field/sector sound order and explicit unsupported frontier.
3. Extend camera/sector numeric and resource-filled domains through actual
   callers. Native audio/listener, device discard and complete driver/Scene
   shutdown remain separate from CPU ownership and projected evidence.
4. Reconstruct railway GameInit/GameDone and remaining authored actor classes
   with genuine gameplay resources. Existing constructor/base Init/LoadData/
   destruction does not supply those later phases.
5. Bind original Game/player/camera ownership and update scheduling to the
   standalone Car lifecycle. Extend foreign actors, occupants, severe damage,
   particles and projection pools through their real callers.
6. Complete floating operands, LET/IF, all 62 parameter cases and Compile/Init/Done;
   connect detector execution, replay, objectives and completion. The authored
   script probe remains FINDACTOR/UnsupportedWorldBackend at PC 3 / source line 4.
7. Validate the complete first-mission sequence against original runtime and
   interactive Rust runs, including restart, cleanup and required presentation.
   Keep CPU/native projections and complete application/GPU parity distinct.

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
