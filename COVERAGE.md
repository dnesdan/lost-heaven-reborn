# Reconstruction coverage

**311 accepted scoped entries / 114 reviewed contracts. Strict full-system gate:
0/12. Whole-function coverage: N/A.** The campaign is not playable and all six
[first-mission gates](docs/GOALS.md) remain open. Static pseudocode, scoped ports,
tests and original-runtime comparisons are independent measures.

The [2026-10-29 integration record](docs/implementation/2026-10-29-pr-integration.md)
pins the code, independent reviews, executed checks, original-data probes and
remaining frontiers. No new original-function credit follows from this batch.
The [model-owned animation record](docs/implementation/2026-10-29-animation-runtime.md)
adds live controller/skin updates and original-resource rendering evidence;
accepted totals remain unchanged.
The [Human/source and base-VM milestone](docs/implementation/2026-10-29-human-program-runtime.md)
retains the preceding Human/source results. The [car/replay/program milestone](docs/implementation/2026-10-29-car-replay-programs.md)
loads both MISE01 cars and reaches 38 actors before kind 20; no new ledger credit is added.

| Subsystem | Source evidence | Recovered behavior | Rust implementation | Validation | Remaining gaps |
| --- | --- | --- | --- | --- | --- |
| Assets / world | PC REA archive/model contracts; pinned mafia-re | Ordered source intake, hierarchy and resource lifetime | Original archives/images, world billboards and inherited mesh sharing | Synthetic malformed/lifetime cases; MISE01 and Salieri world intake | Full SceneOpen, streaming, visibility and remaining derived classes |
| Rendering / materials | [Scene/material contracts](docs/implementation/mafia-re-remediation.md) | Bounded material, LOD, sky and saved-lightmap operations | Owned Scene/Camera, buffers, textures and material clocks | Software-Vulkan GPU tests and actual city captures | Original lighting/effects, general fog, full visual parity |
| Animation | PC instructions; PS2 naming; [owned runtime](docs/implementation/2026-10-29-animation-runtime.md) | Model-owned binding, clock/evaluator order and lazy skin refresh | Actual shared resources/controller and live bound-frame writes; separate host preview | Original producer: 15 states / 345 words; Tick: 12 / 264; actual Tommy/Paulie/Sam GPU motion | Scale, blending/events, companion loading/cache duplication, human selection and scene scheduling |
| Collision / physics | PC instruction/numeric contracts | Scoped matrix and native arithmetic behavior | Contained compatibility math and bounded simulation pieces | Numeric and error-prefix regressions | Complete world collision, contacts and gameplay coupling |
| Vehicles | PC REA ctor/Init/data/dtor; PS2 names/layout; original 428380/4c60a0 | Single base, resource/cleanup order, opaque parameter validity | Actual mission car owners and shared tables; existing standalone path | 17,248 original tail words; both MISE01 banks and knownness match; zero tracked owners | Later GameInit, player/taxi coupling and full world collision/damage |
| Characters / combat | PC Human/Player/Entity instructions; PS2 class names | Constructors, model Init, bounded source reads and derived cleanup | Actual typed owners, embedded Entity CProgram, single reader and original model stop | 3,492 original projected words match; INTRO 28 actors / MISE01 38; zero tracked owners after disposal | Remaining actor kinds, later GameInit, human animation selection, vehicle attachment and combat |
| Traffic / pedestrians | PC tables and source leads | Bounded definitions and ownership | Definition tables and prerequisites | Scoped fixture tests | City population, simulation and mission interactions |
| AI / navigation / police | PC loader contracts; PS2 symbols; pinned pathweb | Navigation intake and reverse flag reset | Installed input into the existing owned graph | Original MISE01: 13,857 nodes / 67,260 links; zero retained allocations | Pathfinding, roads, AI/police and GameInit integration |
| Scripts / cutscenes | PC REA CProgram/cutscene functions; controlled original paths | Reverse global lifecycle, actual reset/start, 102 scalar + 39 no-payload selectors, pool/binding prefix | Actual owned base VM and one recorded manager; three commands execute | 390 zero-param cases; global/subroutine projections; 184 binding words; REP 107,352 bytes | Named parameters/other dispatch, game-global and derived VM, Differences, remaining bindings/tracks and mission scheduling |
| Cameras / input | PC key-mask/camera contracts | Bounded camera and input operations | Authored view inspection and standalone controls | Source tests and bounded X11 input | Original player camera, collisions and gameplay transitions |
| Audio / UI / localization | [Audio contracts](docs/implementation/stage-6-audio.md); PC menu sources | Sound lifetimes, source/cache order and localization intake | Typed sound identities, host mixer and original menu inspection | Synthetic owner/I/O tests; actual-data loading and PCM checks | Driver master update, hardware audio, active HUD/profile and menu transition cleanup |
| Save / load | PC order/helper contracts | Scoped ordering and state helpers | Explicit bounded helpers | Synthetic sequence tests | Full original save compatibility, restart and campaign state |

Latest research uses PC REA/Ghidra and PS2 naming evidence. Controlled original
PC instructions and Rust probes are separate from full-game execution; the
original game was not launched. Normalize matches 74 cases / 296 component
words. Producer and Tick comparisons above cover only their documented
admitted domains; Tick's Update observation is a normalized successful-call
boolean. Linux llvmpipe Vulkan was exercised with the actual model owners.
Native Windows/Vulkan and macOS/Metal remain unverified. Build, logic, rendering,
interactive and original-runtime validation must not be combined into a single
completion percentage. Earlier evidence retains its own revision and scope.
