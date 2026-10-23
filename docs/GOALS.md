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

At `38bd2c4`, original parser/398-row instruction recognition, five C_Vehicle input setters and the triangle-only collision leaf add three bounded contracts: **214 entries / 67 contracts**, **20 static_inspection / 47 not_integrated**, strict **0/12**. Whole-function coverage remains N/A; the checked host placement adapter adds zero entry credit.

Actual authored MISE01 remains FINDACTOR/UnsupportedWorldBackend at PC3 / source line 4; no later line executes. All six first-mission gates remain open. Static GPU/UI and placement checks do not establish driving or mission advancement.

Next are genuine C_Vehicle constructor/Init/cleanup and model/BODY/parameter/wheel ownership, original KLZ binding/grid/primitive contacts, drivetrain helpers and Tick/Engine/Move solver behavior, then application controls/update and camera. Compiler floating operands/LET/IF, all 62 parameter cases and Compile/Init/Done proceed in parallel; actual mission sectors, registration, replay and objectives remain separately required.

Historical Installation integration connects existing menu/static scene archive callers to NativeFiles; its asset-equality and static-rendering receipts remain in [PROGRESS.md](PROGRESS.md). That route supplies neither GPU-worker ownership nor mission-world construction.

Use the [pinned reference audit](REFERENCE_PROJECTS.md) as a function and dependency map for LS3DF and supporting libraries. For each candidate, identify the original binary and entry, callers, layout, state changes and resources; recover it with REA, implement it independently in Rust and compare original execution. Upstream source or instruction-comparison percentages do not establish Rust behavioral coverage. Game.exe and first-mission completion require additional original-game research.

PS2 symbols help recover original names. Keep three origins distinct: an actual original symbol, an upstream-assigned PC correspondence, and a descriptive working name. Record hashes and addresses privately. The supplied PS2 ELF retains linker symbols; the presence of names does not establish full type/member debug information. Verify PC/PS2 layouts, calling conventions and behavior independently before sharing implementations.

The owned model/Joint/Target hierarchy now connects exact-name position/rotation binding, eight-slot controller ownership and one live slot's sequential playback to clock/dispatch and CPU skin upload. Bounded linear/spherical rotation, morph and loaded bounds-cache behavior are available. Regression models and clips test this composition; they do not establish general asset coverage, a constructed gameplay actor or first-mission execution.

Bounded parsed-model lifetime covers admitted model, dummy, unbound Joint and Target teardown, external survivors and stale-playback rejection. Shared mesh/material resources and ten-slot per-object plain LODs compose the standalone IB/VB bridge. Bounded `ITexture` and `C_str` lifetime now includes managed record/queue/pool removal from checked seeded state. Actual bounded managed-record creation now replaces that intake boundary. Bounded Request queue production retains its existing creation/removal and Close composition. Standalone pool unload composes Request/removal from seeded pool state; Close is covered by a separate Rust unit. Bounded standalone managed texture pool release now retires admitted pool state: it clears loaded bits/weak owners under one lock, then releases each pool’s optional system and reverse-visited slot resources, frees its array and resets headers/memory while preserving grow. ReleaseManagedTextures is a pinned mafia-re working name without recovered original PC/PS2 symbol. Full Create/Restore, worker/upload, whole-device shutdown, derived lit/skin/morph LODs and loaded-resource/GPU adapters remain open. Original model/resource File Open, filename sharing, loaded visual destruction and repeated scene load/unload remain open. These standalone resources are not integrated into loaded frames or the application; no new rendered or interactive validation was performed, and all six first-mission gates remain open.

Bounded standalone managed texture Restore now reconstructs admitted system/default pools and reserves the registry through controlled device/storage effects. It composes the accepted Release dependency, selects raw format once, uses returned descriptor Size with signed budgets and wrapping memory arithmetic, grows arrays in 1,024-slot steps and preserves registry entries. RestoreManagedTextures is a pinned mafia-re working name without recovered original PC/PS2 symbol. Full texture Create, worker/upload, real GPU/device implementation, whole-device shutdown, derived LODs, loaded adapters and scene/application integration remain open; no new rendered or interactive validation is claimed.

Fresh REA/Ghidra instruction evidence and actual registry bytes support the bounded replay state and `RECWAITFOREND` bridge. The registry spelling/opcode is original command evidence; replay helper/type names remain descriptive. Actual PS2 linker replay names and the pinned/current upstream trees establish no cinematic replay-owner correspondence. Resource-backed replay Load/Update and actual first-mission ownership remain separate dependencies.

The immediate dependency order is:

1. Own original C_Vehicle constructor/Init/destructor and failure cleanup over real type-9 model/BODY resources, original parameters and wheel/seat structures. Close C_car callback installation and frame/model ownership; detached setters supply none of these defaults.
2. Reconstruct original KLZ binding/grid and remaining primitive/contact queries, dynamic registration, wheel geometry/tyre/suspension/surface state and body-contact callbacks. The triangle leaf alone is not a collision world or contact response.
3. Close drivetrain helpers and real timestep/SetCutTime data, then Tick -> Engine -> Move and wheel/body solver behavior in original numeric order. Preserve six-float Move and original substeps; Update publishes postintegration state rather than replacing dynamics.
4. Connect real vehicle ownership/physics to original application controls/update, player/camera and repeatable original-runtime versus interactive Rust sequences. Static placement is inspection; no simplified physics or prototype driving mode substitutes for this gate.
5. In parallel, close floating DecodeOperand, DecodeLet/DecodeIf, all 62 parameter ownership cases and Compile/Init/Done, then detector GameInit/runtime. Three helpers and complete keyword recognition are prerequisites, not implemented command execution. Actual mission sectors/registration/AE24 flow, replay and objectives remain required for first-mission completion.

Vehicle owner/contact/dynamics reconstruction can proceed before complete mission compilation because the direct C_Vehicle call graph does not intrinsically depend on the compiler. Authored mission acceptance remains separately gated on genuine world construction and original scheduling. Animation/catalog work remains a parallel general-engine dependency; preserve original update order and use actual content as regression inputs.

The [mafia-re/MafiaUnity source audit](REFERENCE_PROJECTS.md#next-general-engine-work) identifies complementary general-engine dependencies: persistent model/scene resource lifetime, mission-world construction, KLZ-backed collision, material states and sector visibility. Research these alongside the sequence above; upstream scene loaders and prototype mechanics do not replace original actor or script behavior.

Work on independent dependencies in parallel using Astra and Sol agents. Integrate verified functions when their real callers and ownership are ready; unavailable paths continue to fail explicitly. Keep library contracts, asset inspection and complete first-mission behavior as separate evidence claims.

## Future graphics

Investigate higher-resolution textures, improved geometry and modern lighting as an optional later mode. Preserve an original presentation mode and local user-owned asset inputs. Texture generation cannot recover missing authentic detail, and remeshing must preserve or deliberately rebuild UVs, materials, joints, morph correspondence, collision and mission anchors. Modernization does not substitute for engine fidelity or the playable-mission gate.

The [asset inventory and enhancement assessment](ASSET_ENHANCEMENT.md) records measured source sizes, parsing limits, topology constraints and a proposed local override workflow. It is research, not an implemented enhanced graphics mode.
