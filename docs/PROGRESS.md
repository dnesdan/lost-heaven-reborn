# Progress

<!-- generated-function-progress:start -->
As of 2026-10-21 · snapshot `RUST-SCOPED-PORTS-20261021-05`.

| Original binary | Static retained pseudocode | Denominator basis | Full contracts | Full Rust ports | Differential runtime verified |
| --- | ---: | --- | ---: | ---: | ---: |
| Game.exe | 4783 / 4,914 (97.33%) | Raw profile inventory | N/A | N/A | N/A |
| rw_data.dll | 6 / N/A | Unknown inventory | N/A | N/A | N/A |
| LS3DF.dll | 1530 / 1,689 (90.59%) | Explicit profile union | N/A | N/A | N/A |
| Setup.exe (supplied configuration) | 172 / 172 (100.00%) | In-image non-thunk procedures | N/A | N/A | N/A |

Strict full-system gate: **0/12**. Binary-level static counts use the named denominator basis and do not combine into a game-completion percentage. Unknown denominators remain N/A. 1 supplied configuration Setup inventory uses 172 in-image non-thunk procedures. Separately, archive installer bootstrap static inventory: **376 / 376 (100.00%)**; this distribution target is not combined with engine-library counts. The campaign is not playable.

Accepted scoped Rust ports: **41 entries across 18 reviewed contracts**. These remain separate from retained pseudocode and whole-function counts.

| Rust contract | Source | Original entries | Native-derived cases | Runtime use | Verified scope |
| --- | --- | ---: | ---: | --- | --- |
| Clock reset and elapsed time | [crates/lhr-engine/src/clock.rs](../crates/lhr-engine/src/clock.rs) | 2 | 20 | Static inspection | Serialized shared u32 clock with the standard sampling/sleep backend; no arbitrary IGraph overrides or caller lifecycle. |
| Scene callback dispatch | [crates/lhr-engine/src/scene_tick.rs](../crates/lhr-engine/src/scene_tick.rs) | 1 | 13 | Static inspection | Immediate mutations and reentry in valid logical collections; animation and actor lifetime remain backend responsibilities. The static inspection uses the constructor-empty Scene path; active scene callbacks remain unbound. |
| Interpolator callback control | [crates/lhr-engine/src/interpolator.rs](../crates/lhr-engine/src/interpolator.rs) | 1 | 31 | No integration recorded | Caller control: 23 callback-observer and 8 scalar projections; evaluator math, poses and full note streams remain backend responsibilities. |
| Game frame-delta modifier | [crates/lhr-engine/src/frame_delta.rs](../crates/lhr-engine/src/frame_delta.rs) | 1 | 22576 | Static inspection | Typed state with masked x87 precision and rounding; original-body comparison excludes FPU status flags, traps and gameplay integration. |
| Generic actor callback dispatch | [crates/lhr-engine/src/actor_tick.rs](../crates/lhr-engine/src/actor_tick.rs) | 1 | 26 | No integration recorded | Visibility and live virtual-call ordering with mutation and reentry; callback implementations, actor ownership and gameplay integration remain open. |
| Game frame coordinator dispatch | [crates/lhr-engine/src/frame_dispatch.rs](../crates/lhr-engine/src/frame_dispatch.rs) | 1 | 1509 | Static inspection | Bounded valid-storage coordinator control flow and arithmetic with synthetic callbacks; required helpers, mission initialization, object lifecycle and full Game integration remain dependencies. |
| LS3DF model tick caller dispatch | [crates/lhr-engine/src/model_tick.rs](../crates/lhr-engine/src/model_tick.rs) | 1 | 7 | Static inspection | Bounded valid model storage caller guard, IPol tick and live finalizer dispatch; the seven Rust checks project selected behaviors from an eleven-case native oracle, not a full eleven-case state differential. Evaluator, finalizer, pose and Game integration excluded. This receipt covers only the static null-IPol path. |
| Selected game-loop control | [crates/lhr-engine/src/selected_loop.rs](../crates/lhr-engine/src/selected_loop.rs) | 1 | 123 | No integration recorded | Bounded selected-loop caller control with explicit fallible callbacks; whole Game ancestry, host adapter integration and invalid native memory behavior remain excluded. |
| 5DS isolated position-channel intake and sampling | [crates/lhr-assets/src/animation.rs](../crates/lhr-assets/src/animation.rs) | 1 | 2684 | No integration recorded | Isolated 5DS position-channel parser, selected reader and evaluator branch within the bounded data/numeric domain; no quaternion/scale math, model binding, IPol lifecycle, hierarchy, skinning or rendered animation parity. |
| Named script lookup callers | [crates/lhr-engine/src/named_script.rs](../crates/lhr-engine/src/named_script.rs) | 2 | 13 | No integration recorded | Mission/global lookup caller control against controlled dependencies; ASCII comparison is bounded, and VM/world/mission effects remain unimplemented. |
| Program reset/start caller | [crates/lhr-engine/src/named_script.rs](../crates/lhr-engine/src/named_script.rs) | 1 | 9 | No integration recorded | Synchronous reset/start caller state mutations before a controlled scheduler observer; full scheduler, VM and program/mission effects are not implemented. |
| LS3DF affine transform primitives | [crates/lhr-engine/src/native_transform.rs](../crates/lhr-engine/src/native_transform.rs) | 7 | 531 | No integration recorded | Seven shipped LS3DF PE exports within the per-call finite-magnitude guard and ordinary round-to-nearest host assumption. 531 accepted cases; one outside-domain case rejected. No all-f32/x87-environment, quaternion-from-matrix, inverse, hierarchy, rendering, or runtime-integration claim. |
| LS3DF quaternion normalization | [crates/lhr-engine/src/native_transform.rs](../crates/lhr-engine/src/native_transform.rs) | 1 | 809 | No integration recorded | S_quat::Normalize at original export RVA 0x30140 in the declared CW 0x003f magnitude-guarded domain; 809 native cases, zero component mismatches in debug and optimized comparisons. Caller must recheck normalized values before subsequent guarded primitives. |
| Shared original configuration behavior | [crates/lhr-engine/src/original_config.rs](../crates/lhr-engine/src/original_config.rs) | 6 | 1577 | No integration recorded | Six bounded shared configuration contracts: five supplied Setup.exe functions and Game.exe defaults. Native projections preserve 63-byte value behavior and selected callbacks/mode lists. No GUI, registry, device enumeration, DirectX creation, installer or whole Setup/Game integration claim. |
| Game script scheduler and bounded dispatcher control | [crates/lhr-engine/src/script_scheduler.rs](../crates/lhr-engine/src/script_scheduler.rs) | 2 | 440 | No integration recorded | Scheduler RVA 0x6ccb0 plus bounded base-dispatcher control at 0x6d3b0; 440 comparison cases and 789 ordered dispatches. This is not complete dispatcher/VM function coverage or full Game integration. |
| Game startup effect and resource behavior | [crates/lhr-engine/src/mission_effects.rs](../crates/lhr-engine/src/mission_effects.rs) | 5 | 6825 | No integration recorded | Five bounded Game.exe entries. Three independent fixture domains: 295 caller-state cases, 640 light traversal/vector cases, and 5,890 resource queries across 155 tables. These domain counts are not a claim of 6,825 full startup executions or complete mission/UI/runtime behavior. |
| LS3DF vector length and quaternion composition helpers | [crates/lhr-engine/src/native_transform.rs](../crates/lhr-engine/src/native_transform.rs) | 2 | 1063 | No integration recorded | Two internal LS3DF helpers named descriptively: vector_length at 0x2f9f0 and SQuat::compose at 0x2fa70. Dedicated fixture contains 532 length calls and 531 composition calls, 1,063 per-function calls rather than unique input rows. Complete original routines were compared on a physical CPU; separate R1 5,000-case paired and R2 1,200-case fresh physical corpora had zero debug/optimized bit mismatches. Declared finite scalar guards and x87 CW 0x003f apply. No ABI or FP-status equivalence, Make/axis-angle, normalization, or runtime integration claim. |
| LS3DF I3D frame transform core | [crates/lhr-engine/src/frame_transform.rs](../crates/lhr-engine/src/frame_transform.rs) | 5 | 502 | Static inspection | Five bounded LS3DF I3D frame methods: 502 native core inputs yielded 10,033 state observations and 1,514,784 compared fields with zero mismatches. Scope covers valid model/mesh trees, LinkTo flags 0 only, and an empty deferred queue. Original-DLL core comparisons used controlled QEMU/Wine calls; bounded arithmetic was separately corroborated on hardware. Production static inspection covers one loaded sipka1 root mesh across 14 states and 343 vertices with zero mismatches; 50 controlled production steps exercised position upload and repeat-upload suppression. No full loader, arbitrary frame variant, refcount, visibility, gameplay, or original renderer parity. |

These rows describe only the reviewed bounded domains. Their native-derived cases do not establish whole-function completion.

| Frozen snapshot | As of | Retained static pseudocode | Scoped Rust entries | Complete systems |
| --- | --- | --- | ---: | ---: |
| `historical-retained-static-20261021` | 2026-10-21 | Game.exe 9 / 4,914 (0.18%); LS3DF.dll 45 / 1,683 (2.67%) | N/A | 0/12 |
| `pc-function-r2-20261021` | 2026-10-21 | Game.exe 55 / 4,914 (1.12%); rw_data.dll 6 / N/A; LS3DF.dll 48 / 1,683 (2.85%) | N/A | 0/12 |
| `RUST-SCOPED-PORTS-20261021-01` | 2026-10-21 | Game.exe 55 / 4,914 (1.12%); rw_data.dll 6 / N/A; LS3DF.dll 48 / 1,683 (2.85%) | 4 | 0/12 |
| `RUST-SCOPED-PORTS-20261021-02` | 2026-10-21 | Game.exe 55 / 4,914 (1.12%); rw_data.dll 6 / N/A; LS3DF.dll 48 / 1,683 (2.85%) | 6 | 0/12 |
| `RUST-SCOPED-PORTS-20261021-03` | 2026-10-21 | Game.exe 72 / 4,914 (1.47%); rw_data.dll 6 / N/A; LS3DF.dll 55 / 1,683 (3.27%) | 8 | 0/12 |
| `RUST-SCOPED-PORTS-20261021-04` | 2026-10-21 | Game.exe 72 / 4,914 (1.47%); rw_data.dll 6 / N/A; LS3DF.dll 55 / 1,683 (3.27%) | 13 | 0/12 |
| `RUST-SCOPED-PORTS-20261021-05` | 2026-10-21 | Game.exe 4783 / 4,914 (97.33%); rw_data.dll 6 / N/A; LS3DF.dll 1530 / 1,689 (90.59%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 41 | 0/12 |

Regenerate locally with `python3 scripts/progress.py --write`; verify without writes with `python3 scripts/progress.py --check`. Versioned snapshots are retained in `docs/progress.json`.
See the [README overview](../README.md) for the compact dashboard.
<!-- generated-function-progress:end -->

Updated 2026-10-21 UTC. The Rust engine displays the original Czech PC menu, static INTRO and a bounded MISE01 neighborhood with the original taxi. It also plays the supported original `sipka1` position animation. This inspection flow does not execute the cinematic or first mission. Full campaign compatibility remains the completion criterion.

| Measure | Verified status and limits |
| --- | --- |
| Linux workspace | `scripts/test.sh` passes 188 Rust tests (62 assets, 102 engine library, 19 application, five renderer), nine REA wrapper tests and 42 progress tests, plus formatting, locked compilation and Clippy. Substantial code milestones receive two sequential independent reviews, including evidence and fidelity. |
| Original assets | Supported A8/A2/A6/A1/AA archives, bounded 4DS/5DS and scene/menu formats. Earlier comparison of 20,229 resources used a private prototype decoder, not independent ground truth; four resources also match independent loose-source hashes. No original files are included. |
| Menu and static world | X11 input verifies keyboard/mouse menu selection, static INTRO, MISE01, orbit, return and pixel-identical repeat loading. The neighborhood selects 64/200 placements within 120 source units: 62 load, two validated billboards are skipped. The taxi has 15 model roots. Lighting, projection and material effects remain approximate. |
| Retained runtime | Clock, delta modifier, coordinator, empty Scene, shared driver time and model dispatch now drive the static inspection. Per-placement poses preserve shared original geometry. A taxi subtree move updates 66 draws; restoration reproduces the initial image exactly. Original mission object population is not recovered. |
| Position animation | Unchanged `sipka1.4ds` and `sipka1.5ds` bind one position-only track to one root mesh. The shared native frame cache and bounded matrix/point operations match 14 before/after-getter states and 343 transformed vertices in controlled original-DLL comparisons. Fifty controlled 16 ms steps animate the actual GPU mesh and return to the initial image. This does not establish character animation, arbitrary hierarchy binding or original rendering parity. |
| Scoped function comparisons | Delta modifier: 22,576 native cases; coordinator: 1,509; selected loop: 123; position evaluator: 2,684 component samples. Seven model tests project selected behavior from an 11-case native oracle. Named script lookup/reset has 13/9 native projections. See the generated contract table for exact domains and review references. |
| Mission research | REA traces startup lifecycle and named script callers. Existing original-game traces confirm dispatch of `game_nightmission`, `use_lightcache`, `policemanager_on` and `enablemap` during MISE01 initialization. Their complete world effects and the required actor/scene materialization are not implemented. The selected original loop and script callers remain separate from playable missions. |
| macOS / Metal | Apple arm64 target compilation is checked; native macOS linking and Metal execution remain untested. |
| Campaign | First-mission gameplay, driving, combat, police and the complete campaign remain unplayable. |

Runtime time steps in the deterministic animation capture are controlled inspection inputs. Original DLL comparisons validate bounded state and numeric behavior; Rust GPU captures validate the new renderer's output, not original visual parity. Source hashes, original addresses, native traces and both review rounds are retained privately. Known original names are used where evidenced; names assigned by researchers are distinguished from original symbols.

## History

| Revision | Milestone | Verification |
| --- | --- | --- |
| [`a077ef2`](https://github.com/dnesdan/lost-heaven-reborn/commit/a077ef25e887b6129c893daf9e5c4885904a2c31) | Standalone Rust engine and loose-model renderer. | 13 Rust tests and nine REA wrapper tests. |
| [`90d2edb`](https://github.com/dnesdan/lost-heaven-reborn/commit/90d2edbed08fa3b8a0227db9a3d45a00fe4b7664) | Lazy reads from selected original DTA archives. | 27 Rust tests; two independent reviews. |
| [`d5e3754`](https://github.com/dnesdan/lost-heaven-reborn/commit/d5e3754b7c26a0b408f051a825114208de2a3b36) | Czech PC menu and bounded static INTRO/MISE01 inspection. | 61 Rust tests; two reviews and input/capture checks. |
| [`0d634fb`](https://github.com/dnesdan/lost-heaven-reborn/commit/0d634fbc9e02941d70de7c2991cdf2e411156b6e) | Recovered frame coordinator and model dispatch. | Native control/state comparisons; two reviews. |
| [`8c1a8ab`](https://github.com/dnesdan/lost-heaven-reborn/commit/8c1a8ab) | Bound static runtime to retained models and GPU updates. | Pixel-stable original scenes, pose upload/restore and two reviews. |
| [`671cf2b`](https://github.com/dnesdan/lost-heaven-reborn/commit/671cf2b) | Bounded 5DS parsing and native position interpolation. | 2,684 bitwise native/Rust component comparisons; two reviews. |
| [`cbeeead`](https://github.com/dnesdan/lost-heaven-reborn/commit/cbeeead) | Named script lookup and synchronous reset/start. | 22 native projections; two reviews. |
| [`ed770ed`](https://github.com/dnesdan/lost-heaven-reborn/commit/ed770ed) | Original single-root position animation through the recovered update path. | Two reviews, independent native state/GPU/CLI comparisons, 145 Rust tests and Apple arm64 compilation. |
| [`4a86c91`](https://github.com/dnesdan/lost-heaven-reborn/commit/4a86c9147dced3c996de94998b1730c63edec857) | Original frame caches and bounded native transforms feed position-animation rendering. | Two independent reviews, 1,514,784 core field comparisons, original-data GPU/CLI captures, 188 Rust tests and Apple arm64 compilation. |

[Dated screenshots](screenshots/README.md) are small actual Rust captures. The former Godot prototype and original-game reference play remain historical evidence, not Rust functionality. Public tests use synthetic fixtures; game data and analysis stay outside Git. Static counts and frozen history are generated above from reviewed metadata; they are not completion percentages.

Use `./scripts/setup.sh`, then `./scripts/test.sh`. No CI workflow is required. See [README](../README.md) for native build, run and repeatable animation-capture commands.
