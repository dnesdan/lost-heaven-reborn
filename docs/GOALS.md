# Reconstruction goals

The immediate gameplay milestone is the original first mission running in the standalone Rust engine, from the original menu through mission completion. The long-term target remains the complete original campaign and engine. A static city, an imported character or a set of isolated native-compatible functions does not satisfy either goal.

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

Use the [pinned reference audit](REFERENCE_PROJECTS.md) as a function and dependency map for LS3DF and supporting libraries. For each candidate, identify the original binary and entry, callers, layout, state changes and resources; recover it with REA, implement it independently in Rust and compare original execution. Upstream source or instruction-comparison percentages do not establish Rust behavioral coverage. Game.exe and first-mission completion require additional original-game research.

PS2 symbols help recover original names. Keep three origins distinct: an actual original symbol, an upstream-assigned PC correspondence, and a descriptive working name. Record hashes and addresses privately. The supplied PS2 ELF retains linker symbols; the presence of names does not establish full type/member debug information. Verify PC/PS2 layouts, calling conventions and behavior independently before sharing implementations.

The actual first-mission `TommyTAXIDRIVER` visual-3 model now loads into the owned Joint/Target hierarchy. Static and independently sampled `breath01a` poses match original morph, skin, frame and bounds outputs. Original 5DS rotation records, bounded rotation evaluation, exact record-to-frame binding and AC access are implemented; these are prerequisites for a character controller, not a playable player.

The next dependency order is:

1. Connect the verified model/Target/morph/skin and position/rotation evaluator paths to original IPol controller ownership and time advancement. Recover missing spherical rotations, scale/notes and blend modes as required by first-mission clips.
2. Recover animation-set/resource cache and required TCK paths, adding A4 access when evidenced. The caller-owned InitAnimIndex sort/cleanup is verified, but the original resource catalog and lifetime remain open.
3. Connect actor/BODY, model and interpolator ownership to animation selection. Keep original animation-code/resource mappings outside any dependency on Game.exe at runtime.
4. Continue original control lock/release, replay, attachments, collision-backed movement, vehicles, camera, pursuit and mission objectives through completion. Compare original-runtime traces and interactive Rust runs of the same sequence.

Work on independent dependencies in parallel using Astra and Sol agents. Integrate verified functions when their real callers and ownership are ready; unavailable paths continue to fail explicitly. Keep library contracts, asset inspection and complete first-mission behavior as separate evidence claims.

## Future graphics

Investigate higher-resolution textures, improved geometry and modern lighting as an optional later mode. Preserve an original presentation mode and local user-owned asset inputs. Texture generation cannot recover missing authentic detail, and remeshing must preserve or deliberately rebuild UVs, materials, joints, morph correspondence, collision and mission anchors. Modernization does not substitute for engine fidelity or the playable-mission gate.

The [asset inventory and enhancement assessment](ASSET_ENHANCEMENT.md) records measured source sizes, parsing limits, topology constraints and a proposed local override workflow. It is research, not an implemented enhanced graphics mode.
