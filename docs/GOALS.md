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

1. Extend constructor-owned CPU Scene lifetime to original Driver CreateFrame,
   viewport override, listener and all native Close/device effects. Immediate
   texture support is bounded; legacy model and weather texture owners remain
   separate. Missing Snow in the supplied data stays null as in the original.
2. Reconstruct genuine preceding mission construction/LoadMap, then authored
   actor classes. The ordered loader requires that context from its caller and
   does not execute or prove it. MISE01's first kind-8 `C_railway` constructor,
   Init, LoadData and GameInit chain is researched but not implemented.
3. Bind the original Game/player/camera lifecycle and update scheduling to the
   standalone Car lifecycle. Extend live foreign actors, occupants, severe
   damage, particles and projection-resource pools as their real callers require.
4. Complete floating operands, LET/IF, all 62 parameter ownership cases and
   Compile/Init/Done; connect detector runtime, replay, objectives and completion.
   The authored script probe remains FINDACTOR/UnsupportedWorldBackend at
   PC 3 / source line 4.
5. Validate the resulting first-mission sequence against original runtime and
   interactive Rust runs, including restart, ownership cleanup and required
   presentation. Keep CPU projections and full application/GPU parity distinct.

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
