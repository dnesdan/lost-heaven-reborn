# Reconstruction coverage

**311 accepted scoped entries / 114 contracts; strict full-system gate 0/12.**
Whole-function coverage remains N/A. These measures do not form an overall
completion percentage. The campaign and first mission remain unplayable.

## Current bounded milestone

The original-data CPU loader now loads, copies and caches static Mesh/Dummy
models with genuine shared resources and cache miss/hit/growth ownership. An
actual-data Rust probe completes 43 records and seven cache entries. Its next
lookup, `base247`, belongs to the original world scene, which has not yet been
loaded by the constructor-only Scene probe.

This is static original-code reconstruction and Rust actual-data testing;
original-runtime cache equivalence is unverified. Genuine SceneOpen with live
sector/portal ownership and loading-progress UI is the next dependency.
The visible application remains standalone driving; the campaign is not playable.

Code `a171632` has two sequential independent source approvals. Source-v4 passes
1202 Rust tests, 9 REA-wrapper/47 progress tests and format/check/Clippy/build checks.
Both profiles match 15 adjacent retained-observation cases; new cache Open and the
expanded loader have no native comparison. The actual-data probe ends at cursor
8019 with zero owned resources after cleanup. Prior standalone controls, three
reloads and four cleanups are carried forward through the byte-identical final
executable; no new UI run is claimed. [Source-bound details](docs/progress.json).

[Current scope and preceding occluder/light record](docs/history/2026-10-25-model-cache-loader.md).

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
[Current screenshot](docs/screenshots/2026-10-25-model-cache-prefix.webp) ·
[full dated validation history](docs/history/2026-10-25-coverage-narrative.md) ·
[progress](docs/PROGRESS.md) · [acceptance goals](docs/GOALS.md).
