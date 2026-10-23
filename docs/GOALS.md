# Reconstruction goals

The goal is a standalone Rust engine that runs the original game's models, animations and complete campaign using the user's own original PC data. Build reusable behavior across evidenced formats and numeric domains; individual models and clips are regression fixtures, not implementation targets. The immediate gameplay milestone is the original first mission, from the original menu through mission completion. A static city, an imported character or isolated native-compatible functions do not satisfy that gate.

| First-mission acceptance gate | Required evidence |
| --- | --- |
| Original startup | Menu starts MISE01; original scripts, actors, resources and update order execute with explicit ownership. |
| Playable control | Original player and taxi respond to input with recovered movement, camera, collision, vehicle and damage behavior. |
| Mission progression | Original replay/cutscene transitions, objectives, required actors, pursuit and events run to the original completion transition. |
| Presentation needed for play | Original models, animation, materials, mission text/HUD and required audio are available; remaining visual differences are recorded. |
| Repeatability | Restart and repeated runs are tested; original-runtime traces and interactive Rust runs exercise the same representative sequence. |
| Standalone execution | User-owned assets suffice; original executables, REA and analysis databases are development inputs only. Linux execution is tested and macOS build compatibility maintained. |

All gates remain open until corresponding evidence is recorded. Current functionality and scoped counts are in [PROGRESS.md](PROGRESS.md); the one full-system coverage table remains [COVERAGE.md](../COVERAGE.md).

## Parity with mafia-re

The selected Installation archive provider now connects existing menu/static scene callers to reconstructed native archive open/read/close behavior while preserving admitted original names, rows and owners. First-archive order is A8, A2, A6, A1, AC, AA, with explicit selected-source errors and no fallback. This host integration adds zero canonical entries or scoped contracts: **177 entries / 59 contracts**, strict **0/12**. Actual asset equality and unchanged static rendering are separate from original-runtime comparisons and playable missions; see [current evidence](PROGRESS.md).

The next first-mission work is the already researched original replay busy predicate and VM wait bridge over live replay state, followed by its real resource, callback, camera and actor ownership. Production texture-worker and surface ownership remain an independent open dependency. The Installation route supplies neither GPU worker ownership nor mission-world construction.

Use the [pinned reference audit](REFERENCE_PROJECTS.md) as a function and dependency map for LS3DF and supporting libraries. For each candidate, identify the original binary and entry, callers, layout, state changes and resources; recover it with REA, implement it independently in Rust and compare original execution. Upstream source or instruction-comparison percentages do not establish Rust behavioral coverage. Game.exe and first-mission completion require additional original-game research.

PS2 symbols help recover original names. Keep three origins distinct: an actual original symbol, an upstream-assigned PC correspondence, and a descriptive working name. Record hashes and addresses privately. The supplied PS2 ELF retains linker symbols; the presence of names does not establish full type/member debug information. Verify PC/PS2 layouts, calling conventions and behavior independently before sharing implementations.

The owned model/Joint/Target hierarchy now connects exact-name position/rotation binding, eight-slot controller ownership and one live slot's sequential playback to clock/dispatch and CPU skin upload. Bounded linear/spherical rotation, morph and loaded bounds-cache behavior are available. Regression models and clips test this composition; they do not establish general asset coverage, a constructed gameplay actor or first-mission execution.

Bounded parsed-model lifetime covers admitted model, dummy, unbound Joint and Target teardown, external survivors and stale-playback rejection. Shared mesh/material resources and ten-slot per-object plain LODs compose the standalone IB/VB bridge. Bounded `ITexture` and `C_str` lifetime now includes managed record/queue/pool removal from checked seeded state. Actual bounded managed-record creation now replaces that intake boundary. Bounded Request queue production retains its existing creation/removal and Close composition. Standalone pool unload composes Request/removal from seeded pool state; Close is covered by a separate Rust unit. Bounded standalone managed texture pool release now retires admitted pool state: it clears loaded bits/weak owners under one lock, then releases each pool’s optional system and reverse-visited slot resources, frees its array and resets headers/memory while preserving grow. ReleaseManagedTextures is a pinned mafia-re working name without recovered original PC/PS2 symbol. Full Create/Restore, worker/upload, whole-device shutdown, derived lit/skin/morph LODs and loaded-resource/GPU adapters remain open. Original model/resource File Open, filename sharing, loaded visual destruction and repeated scene load/unload remain open. These standalone resources are not integrated into loaded frames or the application; no new rendered or interactive validation was performed, and all six first-mission gates remain open.

Bounded standalone managed texture Restore now reconstructs admitted system/default pools and reserves the registry through controlled device/storage effects. It composes the accepted Release dependency, selects raw format once, uses returned descriptor Size with signed budgets and wrapping memory arithmetic, grows arrays in 1,024-slot steps and preserves registry entries. RestoreManagedTextures is a pinned mafia-re working name without recovered original PC/PS2 symbol. Full texture Create, worker/upload, real GPU/device implementation, whole-device shutdown, derived LODs, loaded adapters and scene/application integration remain open; no new rendered or interactive validation is claimed.

Fresh REA/Ghidra static evidence `PC-GAME-REPLAY-DEPENDENCIES-20261023` establishes three replay dependencies: the 32-byte busy predicate at RVA `0x19e2d0`, 450-byte state setter at `0x1989a0`, and 3,089-byte update at `0x199610`. These are descriptive working names, with no established PS2 correspondence. The VM wait arm advances only when busy is false; the update decrements the load barrier once per update, while camera sentinel handling can stop replay independently. These are static findings, with zero new runtime comparisons or implementation credit. Complete replay resources, live callback ownership, camera/actor integration and first-mission execution remain open.

The next dependency order is:

1. Extend animation dependencies across admitted data: weighted multi-slot evaluation, scale/note channels, remaining numeric domains and original notification boundaries. Preserve controller ownership and original update order as coverage grows.
2. Recover animation-set/resource cache, animation-machine construction and required TCK paths, adding A4 access when evidenced. InitAnimIndex sort/cleanup and GetTrack over caller-owned records are bounded prerequisites; the original catalog, locale-comparison backend and resource lifetime remain open.
3. Connect actor/BODY, model and controller ownership to original animation selection. Keep original animation-code/resource mappings outside any dependency on Game.exe at runtime.
4. Implement the researched replay state and VM-wait bridge in dependency order, then continue original control lock/release, attachments, collision-backed movement, vehicles, camera, pursuit and mission objectives through completion. Compare original-runtime traces and interactive Rust runs of the same sequence.

The [mafia-re/MafiaUnity source audit](REFERENCE_PROJECTS.md#next-general-engine-work) identifies complementary general-engine dependencies: persistent model/scene resource lifetime, mission-world construction, KLZ-backed collision, material states and sector visibility. Research these alongside the sequence above; upstream scene loaders and prototype mechanics do not replace original actor or script behavior.

Work on independent dependencies in parallel using Astra and Sol agents. Integrate verified functions when their real callers and ownership are ready; unavailable paths continue to fail explicitly. Keep library contracts, asset inspection and complete first-mission behavior as separate evidence claims.

## Future graphics

Investigate higher-resolution textures, improved geometry and modern lighting as an optional later mode. Preserve an original presentation mode and local user-owned asset inputs. Texture generation cannot recover missing authentic detail, and remeshing must preserve or deliberately rebuild UVs, materials, joints, morph correspondence, collision and mission anchors. Modernization does not substitute for engine fidelity or the playable-mission gate.

The [asset inventory and enhancement assessment](ASSET_ENHANCEMENT.md) records measured source sizes, parsing limits, topology constraints and a proposed local override workflow. It is research, not an implemented enhanced graphics mode.
