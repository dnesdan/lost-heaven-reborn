# Reconstruction goals

The goal is a standalone Rust engine that runs the original game's models, animations and complete campaign using the user's own original PC data. Build reusable behavior across evidenced formats and numeric domains; individual models and clips are regression fixtures, not implementation targets. The immediate gameplay milestone is the original first mission, from the original menu through mission completion. A static city, an imported character or isolated native-compatible functions do not satisfy that gate.

| First-mission acceptance gate | Required evidence |
| --- | --- |
| Original startup | Menu starts MISE01; original scripts, actors, resources and update order execute with explicit ownership. |
| Playable control | Original player and taxi respond to input with recovered movement, camera, collision, vehicle and damage behavior. |
| Mission progression | Original replay/cutscene transitions, objectives, required actors, pursuit and events run to the original completion transition. |
| Presentation needed for play | Original models, animation, materials, mission text/HUD and required audio are available; remaining visual differences are recorded. |
| Repeatability | Restart and repeated runs are tested; original-runtime traces and interactive Rust runs exercise the same representative sequence. |
| Standalone execution | User-owned assets suffice; original executables, REA and analysis databases are development inputs only. Linux execution is tested. The current Apple cross-check is blocked by the missing AudioUnit SDK; native macOS and Metal remain untested. |

All gates remain open until corresponding evidence is recorded. Current functionality and scoped counts are in [PROGRESS.md](PROGRESS.md); the one full-system coverage table remains [COVERAGE.md](../COVERAGE.md).

## Parity with mafia-re

Standalone original-data driving now composes the Car constructor, Init,
GameInit, Tick/Engine/Move/Update and explicit GameDone/destruction over one live
owner. Original static collision, material/tyre tables, WAV sound and host input/
chase presentation are connected. The accepted canonical-entry ledger is now
311 entries / 114 contracts after the separate weather/default dependencies;
full-system verification remains 0/12.

The authored MISE01 program probe still stops at FINDACTOR/UnsupportedWorldBackend,
PC 3 / source line 4. All six first-mission gates remain open. Complete original
Game/Scene/player construction, foreign dynamic actors, live particle/projection
pools and remaining severe-damage effects are separate dependencies. Standalone
source-parameter flags 2 and audio no longer form the initial driving frontier.

Historical Installation integration connects existing menu/static scene archive callers to NativeFiles; its asset-equality and static-rendering receipts remain in [PROGRESS.md](PROGRESS.md). That route supplies neither GPU-worker ownership nor mission-world construction.

Use the [pinned reference audit](REFERENCE_PROJECTS.md) as a function and dependency map for LS3DF and supporting libraries. For each candidate, identify the original binary and entry, callers, layout, state changes and resources; recover it with REA, implement it independently in Rust and compare original execution. Upstream source or instruction-comparison percentages do not establish Rust behavioral coverage. Game.exe and first-mission completion require additional original-game research.

PS2 symbols help recover original names. Keep three origins distinct: an actual original symbol, an upstream-assigned PC correspondence, and a descriptive working name. Record hashes and addresses privately. The supplied PS2 ELF retains linker symbols; the presence of names does not establish full type/member debug information. Verify PC/PS2 layouts, calling conventions and behavior independently before sharing implementations.

The owned model/Joint/Target hierarchy now connects exact-name position/rotation binding, eight-slot controller ownership and one live slot's sequential playback to clock/dispatch and CPU skin upload. Bounded linear/spherical rotation, morph and loaded bounds-cache behavior are available. Regression models and clips test this composition; they do not establish general asset coverage, a constructed gameplay actor or first-mission execution.

Bounded parsed-model lifetime covers admitted model, dummy, unbound Joint and Target teardown, external survivors and stale-playback rejection. Shared mesh/material resources and ten-slot per-object plain LODs compose the standalone IB/VB bridge. Bounded `ITexture` and `C_str` lifetime now includes managed record/queue/pool removal from checked seeded state. Actual bounded managed-record creation now replaces that intake boundary. Bounded Request queue production retains its existing creation/removal and Close composition. Standalone pool unload composes Request/removal from seeded pool state; Close is covered by a separate Rust unit. Bounded standalone managed texture pool release now retires admitted pool state: it clears loaded bits/weak owners under one lock, then releases each pool’s optional system and reverse-visited slot resources, frees its array and resets headers/memory while preserving grow. ReleaseManagedTextures is a pinned mafia-re working name without recovered original PC/PS2 symbol. Admitted loaded plain-object frames now connect to shared CPU mesh/material/texture ownership and explicit cleanup, including retained external mesh survivors and repeated Close controls. Full native Create/worker/upload, whole-device shutdown, derived lit/skin/morph resources and GPU/application adapters remain open, as do full native loader/callback/filename parity and complete repeated scene load/unload. Bounded Rust filename sharing and reopen ownership are implemented. Those standalone ownership checks establish no original-renderer parity; current car-inspection rendering and camera/reload evidence are scoped separately. All six first-mission gates remain open.

Bounded standalone managed texture Restore now reconstructs admitted system/default pools and reserves the registry through controlled device/storage effects. It composes the accepted Release dependency, selects raw format once, uses returned descriptor Size with signed budgets and wrapping memory arithmetic, grows arrays in 1,024-slot steps and preserves registry entries. RestoreManagedTextures is a pinned mafia-re working name without recovered original PC/PS2 symbol. Admitted plain-object loaded resources now connect to CPU ModelArena ownership. Full native texture Create/worker/upload, real GPU/device implementation, whole-device shutdown, derived lit/skin/morph resources and application/mission integration remain open; these standalone resource contracts establish no original-renderer parity.

Fresh REA/Ghidra instruction evidence and actual registry bytes support the bounded replay state and `RECWAITFOREND` bridge. The registry spelling/opcode is original command evidence; replay helper/type names remain descriptive. Actual PS2 linker replay names and the pinned/current upstream trees establish no cinematic replay-owner correspondence. Resource-backed replay Load/Update and actual first-mission ownership remain separate dependencies.

Weather reset and sector sound defaults are reconstructed and compared separately.
They are not yet bound to live Scene construction; viewport and actual Snow
texture creation/shared destruction remain the next constructor dependencies.

The immediate dependency order is:

1. Integrate the recovered standalone Car GameInit/driving lifecycle into the original Game, player and authored actor lifecycle. Complete Scene viewport/weather/shared Snow construction beyond 0x48e29 and the thirteen actual Scene Close effects with their correct resource identities, then original File Open/reload and mission-world ownership. Standalone source-parameter flags 2 and original WAV audio are now bound; live particle and projection pools remain open.
2. Extend the standalone original-world Tick -> Engine -> six-float Move -> DoWheelsCollision composition to live foreign dynamic actors, occupants and remaining severe-damage effects. Preserve original solver order, canonical ownership and the source-bound native profiles. Replace the host chase/input bridge with original player/camera scheduling and connect repeatable full-game driving sequences.
3. Complete floating DecodeOperand, DecodeLet/DecodeIf, all 62 parameter ownership cases and Compile/Init/Done, then detector runtime and genuine mission scene/actor registration. Replay, objectives and original scheduling remain required. Actual authored MISE01 remains FINDACTOR/UnsupportedWorldBackend at PC 3 / source line 4.

Standalone vehicle initialization and CPU ownership can be exercised before complete mission compilation; they do not supply the missing enclosing Game/world/driver and authored mission paths. Original models are regression inputs for general behavior. Animation and rendering resource dependencies remain independent work.

The [mafia-re/MafiaUnity source audit](REFERENCE_PROJECTS.md#next-general-engine-work) identifies complementary general-engine dependencies: persistent model/scene resource lifetime, mission-world construction, KLZ-backed collision, material states and sector visibility. Research these alongside the sequence above; upstream scene loaders and prototype mechanics do not replace original actor or script behavior.

Work on independent dependencies in parallel using Astra and Sol agents. Integrate verified functions when their real callers and ownership are ready; unavailable paths continue to fail explicitly. Keep library contracts, asset inspection and complete first-mission behavior as separate evidence claims.

## Future graphics

Investigate higher-resolution textures, improved geometry and modern lighting as an optional later mode. Preserve an original presentation mode and local user-owned asset inputs. Texture generation cannot recover missing authentic detail, and remeshing must preserve or deliberately rebuild UVs, materials, joints, morph correspondence, collision and mission anchors. Modernization does not substitute for engine fidelity or the playable-mission gate.

The [asset inventory and enhancement assessment](ASSET_ENHANCEMENT.md) records measured source sizes, parsing limits, topology constraints and a proposed local override workflow. It is research, not an implemented enhanced graphics mode.
