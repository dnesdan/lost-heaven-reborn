# Reconstruction coverage

**311 accepted scoped entries / 114 contracts; strict full-system gate 0/12.**
Whole-function coverage remains N/A. These measures do not form an overall
completion percentage. The campaign and first mission remain unplayable.

## Current bounded milestone

The staged source-based merge connects original-data loaders and image intake,
live material parameters and camera-selected model LOD, authored view
orientation, audio lifetime, archive/write IO and language selection. Lit objects
use the unconstructed cyan source branch. Saved lightmaps, scene `LMAP` payload
association and original lighting/visibility remain open.

Accepted code `a4dee6d` passes the required gate and **1310 Rust tests
in both debug and release**, including both unchanged x87 tests without
exclusions; 9 Python boundary and 47 progress tests pass. Seven opt-in GPU
checks pass on Mesa llvmpipe Vulkan. New standalone W/A/D/S/Space, three
reloads and Escape finish with four cleanups and seven zero CPU counters.

The visible application remains inspection and standalone driving. Source tests,
GPU checks and actual UI input have separate scopes; they do not establish
original-game image/audio parity, complete SceneOpen/MissionOpen or a playable
mission. [Stage scope and validation](docs/implementation/mafia-re-remediation.md) ·
[preceding current narrative](docs/history/2026-10-26-pr-merge-validation.md).

## Strict system registry

Each row needs original evidence, implementation and relevant runtime validation.
Partial support below does not pass a system gate.

| System | Current bounded support | Remaining gate | Verified |
| --- | --- | --- | --- |
| Assets and world streaming | DTA/model/scene intake; nearby world geometry | Full Game/world streaming and mission ownership | No |
| Rendering and materials | Software Vulkan, live unlit materials/model LOD; unconstructed Lit | Original lighting/material/GPU behavior; Metal | No |
| Animation | Owned hierarchy, one sequential controller slot, CPU skinning | Blending, scale/notes and gameplay selection | No |
| Collision and physics | Original-road and bounded dynamic contacts | General actor/world response breadth | No |
| Vehicles | Standalone lifecycle, driving, damage and cleanup | Player/mission ownership, occupants, severe-damage paths | No |
| Characters and combat | Base/source-only and bounded railway lifecycle; ordered loader | Authored derived actors and combat | No |
| Traffic and pedestrians | Resource presence only | Spawn, routing and simulation | No |
| AI, navigation and police | Selected manager initialization/reset | Resource-backed AI, navigation and pursuit | No |
| Scripts and cutscenes | Parsing, bounded scheduler/lookup/replay dependencies | Compiler, original scheduling, objectives and completion | No |
| Camera and input | Host driving controls/chase view over recovered setters | Original player/camera scheduling | No |
| Audio, UI and localization | Selected data-language menu, PCM16 WAV mixing/capture | Codecs, HUD, profile/settings and hardware verification | No |
| Save and load | No persistence implementation | Schema, save/load and recovery | No |

Actual UI evidence is separate from native CPU projections. Host CW/MXCSR,
original rendering/audio parity and native macOS/Metal are unclaimed.
[Current screenshot](docs/screenshots/2026-10-26-pr-merge-validation.webp) ·
[full dated validation history](docs/history/2026-10-25-coverage-narrative.md) ·
[progress](docs/PROGRESS.md) · [acceptance goals](docs/GOALS.md).
