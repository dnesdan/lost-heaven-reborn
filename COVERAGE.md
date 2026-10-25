# Reconstruction coverage

**311 accepted scoped entries / 114 contracts; strict full-system gate 0/12.**
Whole-function coverage remains N/A. These measures do not form an overall
completion percentage. The campaign and first mission remain unplayable.

## Current bounded milestone

The original-data CPU world loader now owns 1415 scene frames, five sectors
and 14 portals with 36 hull planes. It supplies the real `base247` object before
the ordered scene2 loader, which completes 43 records and then reaches the
required `OBJ_` → `LMAP` visual replacement at cursor 8013 (Game `543a4b`).

Live sector/portal ownership, spatial queries, mutable-key buffer trees and
unequal-size alpha maps now share the genuine CPU resource path. Original
SceneOpen presentation, the loading-progress UI and `LMAP` replacement remain
open. This is Rust actual-data execution; native composed-loader parity and
complete MissionOpen are unverified. The visible application remains standalone
driving; the campaign is not playable.

Code `085f31e` has two sequential independent source approvals. Source-v4 passes
1222 Rust tests, 9 REA-wrapper/47 progress tests and format/check/Clippy/build checks.
Both Rust profiles match 15 retained buffer trajectories/92 phases; this is separate
from the actual-data CPU world probes. Those probes reach `LMAP` and release all
tracked resources. New standalone W/A/D/S/Space, three reloads and four cleanups
have seven zero CPU counters. Neither probe nor capture establishes native
SceneOpen, loading UI, mission execution or original rendering parity.

[Current scope and preceding cache record](docs/history/2026-10-25-world-sector-loader.md).

## Strict system registry

Each row needs original evidence, implementation and relevant runtime validation.
Partial support below does not pass a system gate.

| System | Current bounded support | Remaining gate | Verified |
| --- | --- | --- | --- |
| Assets and world streaming | DTA/model/scene intake; nearby world geometry | Full Game/world streaming and mission ownership | No |
| Rendering and materials | Software Vulkan, unlit first LOD | Original lighting/material/GPU behavior; Metal | No |
| Animation | Owned hierarchy, one sequential controller slot, CPU skinning | Blending, scale/notes and gameplay selection | No |
| Collision and physics | Original-road and bounded dynamic contacts | General actor/world response breadth | No |
| Vehicles | Standalone lifecycle, driving, damage and cleanup | Player/mission ownership, occupants, severe-damage paths | No |
| Characters and combat | Base/source-only and bounded railway lifecycle; ordered loader | Authored derived actors and combat | No |
| Traffic and pedestrians | Resource presence only | Spawn, routing and simulation | No |
| AI, navigation and police | Selected manager initialization/reset | Resource-backed AI, navigation and pursuit | No |
| Scripts and cutscenes | Parsing, bounded scheduler/lookup/replay dependencies | Compiler, original scheduling, objectives and completion | No |
| Camera and input | Host driving controls/chase view over recovered setters | Original player/camera scheduling | No |
| Audio, UI and localization | Czech menu, PCM16 WAV mixing/capture | Codecs, HUD, profile/settings and hardware verification | No |
| Save and load | No persistence implementation | Schema, save/load and recovery | No |

Actual UI evidence is separate from native CPU projections. Host CW/MXCSR,
original rendering/audio parity and native macOS/Metal are unclaimed.
[Current screenshot](docs/screenshots/2026-10-25-world-sector-prefix.webp) ·
[full dated validation history](docs/history/2026-10-25-coverage-narrative.md) ·
[progress](docs/PROGRESS.md) · [acceptance goals](docs/GOALS.md).
