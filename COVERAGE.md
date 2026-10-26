# Reconstruction coverage

**311 accepted scoped entries / 114 contracts; strict full-system gate 0/12.**
Whole-function coverage remains **N/A**. These measures do not form a game
completion percentage. The campaign is not playable and all six first-mission
gates remain open.

## Current bounded milestone — 2026-10-26

Source [`dddaf539`](https://github.com/dnesdan/lost-heaven-reborn/commit/dddaf539c9b874b9cf7a67d866532d901729df18)
connects actual `OBJ_` → `LMAP` replacement, saved lighting, UV2/atlas resources,
Scene/active-Camera backdrop/main/near passes and portal/occluder visibility.
Guarded owners preserve interrupted prefixes. The existing bounded host cache
selector supplies **907 actual scene-owned models in the measured driving run**
through shared model caches; it does not establish the original city-streaming controller.

The required gate passes: **1362 Rust tests in debug and release**, **1374 with
12 opt-in GPU checks**, 9 Python boundary tests, 47 progress tests and Clippy.
An actual-data CPU driving run exercises 240 steps, audio and GameDone, then
releases all tracked resources. The actual saved-atlas GPU counterfactual changes
53 pixels and restoration recovers the complete RGBA image byte-for-byte.
The fresh windowed run passes W/A/D/S/Space, three reloads and Escape; four
cleanups each reach seven zero CPU counters.

Actual MISE01 world intake retains 1415 frames, 5 sectors and 14 portals. Scene2
completes **44 records** and stops at cursor **18693**, unsupported
`FRAME_DUMMY` (6), `detector_konec`. Road and sky remain dark or absent; the
cause is unresolved. General shader/lighting, complete SceneOpen/MissionOpen,
original scheduling and native image/audio parity remain unverified. This
milestone executes no original game executable/function and adds no ledger credit.

## Strict system registry

Each gate requires original evidence, implementation and relevant runtime
validation. Bounded support does not pass a complete-system gate.

| System | Current bounded support | Remaining gate | Verified |
| --- | --- | --- | --- |
| Assets and world streaming | DTA/model/world intake; actual models from admitted host cache selections | Original Game streaming and complete mission ownership | No |
| Rendering and materials | Owned three-pass Scene/Camera drawing, saved lighting/UV2/atlases, portal/occluder visibility, selected material/light branches | General renderer/shader behavior, unresolved road/sky presentation, Metal | No |
| Animation | Owned hierarchy, one sequential controller slot, CPU skinning | Blending, scale/notes and gameplay selection | No |
| Collision and physics | Original-road and bounded dynamic contacts | General actor/world response breadth | No |
| Vehicles | Standalone lifecycle, driving, damage and guarded cleanup | Player/mission ownership, occupants, severe-damage paths | No |
| Characters and combat | Base/source-only and bounded railway lifecycle; ordered loader | Authored derived actors and combat | No |
| Traffic and pedestrians | Resource presence only | Spawn, routing and simulation | No |
| AI, navigation and police | Selected manager initialization/reset | Resource-backed AI, navigation and pursuit | No |
| Scripts and cutscenes | Parsing, bounded scheduler/lookup/replay dependencies | Compiler, original scheduling, objectives and completion | No |
| Camera and input | Actual active-camera matrices/viewport feed drawing; host driving controls | Original player/camera scheduling and remaining driver effects | No |
| Audio, UI and localization | Selected data-language menu, PCM16 WAV mixing/capture | Codecs, HUD, profile/settings and hardware verification | No |
| Save and load | No persistence implementation | Schema, save/load and recovery | No |

Source tests, GPU checks, actual-data CPU probes and UI input have separate
scopes. Native macOS/Metal remains untested. [Current scope](docs/implementation/mafia-re-remediation.md) ·
[progress](docs/PROGRESS.md) · [acceptance goals](docs/GOALS.md) ·
[complete predecessor](docs/history/2026-10-26-scene-render-predecessor.md#coverage).
