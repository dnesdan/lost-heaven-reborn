# Reconstruction coverage

**311 accepted scoped entries / 114 contracts; strict full-system gate 0/12.**
Whole-function coverage remains N/A. These measures do not form an overall
completion percentage. The campaign and first mission remain unplayable.

## Current bounded milestone

The actual driver type-13 factory now constructs the Scene, applies its original
viewport setter and appends a borrowed registry pointer without AddRef. Scene
destruction removes that pointer; driver member-storage cleanup frees retained
registry capacity after all Scenes are gone. This does not establish full driver
shutdown, native sound-listener behavior or exceptional-unwind equivalence.

Kind-8 `C_railway` supports constructor defaults, existing base Init, guarded
25-byte LoadData and destructor cleanup. The actual model StopAnimation guard
reads `model+0x150`; the admitted null IPolSet performs no controller writes.
Controller ownership remains untouched. Railway GameInit/GameDone and
their gameplay allocations remain unimplemented.

The existing-Scene `OriginalLoadMap` initializer performs actual frame enumeration
and publishes existing primary/backdrop/omni references, excluding Near. It creates
no sectors. Callers still supply a genuine Scene; complete mission
construction, top-object loading, model cache and Mission Open remain open.
Player/camera ownership, authored actor GameInit and script scheduling are absent.

Earlier immediate texture/cache support remains bounded to its documented domain.
Legacy model textures and canonical weather textures are separate owners. Original
data has no Snow texture, preserving the native null outcome. CPU Close still
reports `native_device_virtual14_executed=false`; no complete native Close, device
or GPU parity is claimed. The script probe remains FINDACTOR/UnsupportedWorldBackend,
PC 3 / source line 4. All six first-mission gates remain open.

Code `b62dd2c` has two sequential independent non-author source approvals.
The full gate passes 1113 Rust tests, 9 REA-wrapper and 47 progress tests,
plus formatting, locked checking, Clippy and the application build.
Debug UI checks cover three reloads and four zero-owned CPU/driver cleanups.
Physical CPU comparisons match 10 driver and 25 railway cases in both Cargo
profiles; O0/O3 agree. Six native-only railway frontiers are excluded.
Separately, qemu/RSP observes 8 world cases / 36 publications; four debug Rust
world tests include eight schedule observations, without physical/dual-profile parity.
Driver member-storage cleanup has primary/unit/UI evidence, not native comparison.

[Detailed source-bound counts and limits](docs/progress.json).

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
[Current screenshot](docs/screenshots/2026-10-25-driver-scene.webp) ·
[full dated validation history](docs/history/2026-10-25-coverage-narrative.md) ·
[progress](docs/PROGRESS.md) · [acceptance goals](docs/GOALS.md).
