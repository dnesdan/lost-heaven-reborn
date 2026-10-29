# Reconstruction coverage

**311 accepted scoped entries / 114 reviewed contracts. Strict full-system gate:
0/12. Whole-function coverage: N/A.** The campaign is not playable and all six
[first-mission gates](docs/GOALS.md) remain open. Static pseudocode, scoped ports,
tests and original-runtime comparisons are independent measures.

The [2026-10-29 integration record](docs/implementation/2026-10-29-pr-integration.md)
pins the code, independent reviews, executed checks, original-data probes and
remaining frontiers. No new original-function credit follows from this batch.

| Subsystem | Source evidence | Recovered behavior | Rust implementation | Validation | Remaining gaps |
| --- | --- | --- | --- | --- | --- |
| Assets / world | PC REA archive/model contracts; pinned mafia-re | Ordered source intake, hierarchy and resource lifetime | Original archives/images, world billboards and inherited mesh sharing | Synthetic malformed/lifetime cases; MISE01 and Salieri world intake | Full SceneOpen, streaming, visibility and remaining derived classes |
| Rendering / materials | [Scene/material contracts](docs/implementation/mafia-re-remediation.md) | Bounded material, LOD, sky and saved-lightmap operations | Owned Scene/Camera, buffers, textures and material clocks | Software-Vulkan GPU tests and actual city captures | Original lighting/effects, general fog, full visual parity |
| Animation | PC contracts; PS2 naming; [scoped ports](docs/implementation/parity-continuation.md) | Bounded clocks, skeleton and MRPH state | Initial poses, owned deformation and live material state | Synthetic state/GPU tests; retained focused MRPH captures | Full controllers, blending/events and original-content animation |
| Collision / physics | PC instruction/numeric contracts | Scoped matrix and native arithmetic behavior | Contained compatibility math and bounded simulation pieces | Numeric and error-prefix regressions | Complete world collision, contacts and gameplay coupling |
| Vehicles | PC vehicle contracts; retained runtime traces | Scoped driving, damage and sound operations | Standalone owned car path | Native-derived regressions and bounded standalone input probes | Mission taxi/player binding and full collision/damage validation |
| Characters / combat | Ordered actor source contracts | Reached actor construction/load prefixes | Existing actor owners and explicit unsupported kinds | MISE01 reaches 27 actors, kind 27 boundary | C_entity, player/combat and remaining actor factories |
| Traffic / pedestrians | PC tables and source leads | Bounded definitions and ownership | Definition tables and prerequisites | Scoped fixture tests | City population, simulation and mission interactions |
| AI / navigation / police | PC loader contracts; PS2 symbols; pinned pathweb | Navigation intake and reverse flag reset | Installed input into the existing owned graph | Original MISE01: 13,857 nodes / 67,260 links; zero retained allocations | Pathfinding, roads, AI/police and GameInit integration |
| Scripts / cutscenes | PC control-VM contracts; pinned C_program | Scoped VM operations, source intake and compiler scanning | Retained sources and explicit decoder/backend boundary | Synthetic control/error/keyword tests | DecodeParams, production instruction owner, interpreter and scheduling |
| Cameras / input | PC key-mask/camera contracts | Bounded camera and input operations | Authored view inspection and standalone controls | Source tests and bounded X11 input | Original player camera, collisions and gameplay transitions |
| Audio / UI / localization | [Audio contracts](docs/implementation/stage-6-audio.md); PC menu sources | Sound lifetimes, source/cache order and localization intake | Typed sound identities, host mixer and original menu inspection | Synthetic owner/I/O tests; actual-data loading and PCM checks | Driver master update, hardware audio, active HUD/profile and menu transition cleanup |
| Save / load | PC order/helper contracts | Scoped ordering and state helpers | Explicit bounded helpers | Synthetic sequence tests | Full original save compatibility, restart and campaign state |

Latest native research uses PC REA/Ghidra and a separate PS2 symbol check;
no original executable ran in this milestone. Linux llvmpipe Vulkan was exercised.
Native Windows/Vulkan and macOS/Metal remain unverified. Build, logic, rendering,
interactive and original-runtime validation must not be combined into a single
completion percentage. Earlier evidence retains its own revision and scope.
