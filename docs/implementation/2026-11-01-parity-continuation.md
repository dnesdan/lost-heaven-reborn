# Nine audit follow-ups and the remaining gameplay chain

Updated 2026-11-01. This is the continuation of the fresh code review, not a
campaign completion claim. The public baseline for this record is
`ba8ed22d17a5722e810bf1c8cb6f6b8dcbb81921` on `rust-engine`.
The pinned mafia-re reference is
[`6c9f71c9bb52da7188eced90f9f732e7290657e9`](https://github.com/Marvisak/mafia-re/tree/6c9f71c9bb52da7188eced90f9f732e7290657e9).
All six [first-mission gates](../GOALS.md) remain open. Loading data, isolated
instruction comparisons and a working diagnostic view have distinct scopes.

## Delivered changes

| PR | Actual change | Validation scope |
| --- | --- | --- |
| [#58](https://github.com/dnesdan/lost-heaven-reborn/pull/58) | Corrected source/name disposal guards, retained callback observations and named command compilation. | Workspace tests and actual-source unwind probe; two independent source reviews. |
| [#59](https://github.com/dnesdan/lost-heaven-reborn/pull/59) | Frame/actor lookup uses live reads and writes the caller's actual VM reference banks. | Workspace and focused lookup tests; two independent source reviews. |
| [#60](https://github.com/dnesdan/lost-heaven-reborn/pull/60) | Source-owned `CProgram::Process` dispatches FINDFRAME/FINDACTOR and prevents replay after uncertain destructive effects. | 1,979 ordinary Rust and 106 Python tests; all-feature Clippy; actual-source unwind probe; two independent source reviews. |
| [#61](https://github.com/dnesdan/lost-heaven-reborn/pull/61) | Stable saved-lighting snapshots and persistent GPU uploads; successful draw/lighting changes prune stale cache entries while live draws retain their GPU owners. | 1,980 ordinary Rust and 106 Python tests, all-feature check, four explicit software-Vulkan GPU cases; two independent source reviews. |
| [#62](https://github.com/dnesdan/lost-heaven-reborn/pull/62) | Reached scene2 crash effects mutate the canonical model used by rendering and teardown, with source/world/arena/lifetime checks. | 1,985 ordinary Rust and 106 Python tests, 1,988 all-feature tests, strict all-feature Clippy and all 25 explicit software-Vulkan GPU cases; two independent source reviews. |
| [#63](https://github.com/dnesdan/lost-heaven-reborn/pull/63) | Valid empty model-cache copies preserve finalization; common userdata cleanup follows actual retained descendants. | 1,991 ordinary Rust and 106 Python tests, strict all-feature Clippy, 27 focused cases and an original-data load-prefix probe; two independent source reviews. |
| [#64](https://github.com/dnesdan/lost-heaven-reborn/pull/64) | The existing mission content owner runs its actual programs against its actual ActorWorld and the caller's live Scene/ModelArena. | 2,000 ordinary Rust and 106 Python tests, 2,003 all-feature tests, strict all-feature Clippy and nine actual-owner fixtures; two independent source reviews. |
| [#65](https://github.com/dnesdan/lost-heaven-reborn/pull/65) | Genuine TrafficGenerator source construction, Init, versioned LoadData and teardown, with signed PC32 chunk reads. | 2,017 ordinary Rust and 106 Python tests, 2,020 all-feature tests, strict all-feature Clippy, 17 focused cases and a fresh original Morello load-prefix probe; two independent source reviews. |
| [#66](https://github.com/dnesdan/lost-heaven-reborn/pull/66) | General companion animation loading and controller duplication share the actual animation resource; reached scale tracks use the evidenced PC24 arithmetic. | 2,025 ordinary Rust and 106 Python tests, strict all-feature Clippy, focused parser/controller/cache cases and fresh menu/Salieri load-prefix probes. Of 26 isolated original-instruction CPU cases, 22 admitted active scale tuples are compared bit for bit by Rust fixtures; two independent source reviews. |
| [#67](https://github.com/dnesdan/lost-heaven-reborn/pull/67) | The retained mission session exposes Process and named execution over those same live owners and a single validated, callback-mutable Scene selection. | Six installed-source integration cases, 2,031 ordinary Rust and 106 Python tests, 2,034 all-feature tests and strict all-feature Clippy; two independent source reviews. |

These are executed Rust and, where explicitly listed, software-Vulkan results at
the exact source trees recorded in each PR. The ordinary and all-feature counts
are separate configurations of overlapping tests, not additive coverage. The
isolated CPU probes execute original PC instructions with controlled surrounding
owners; they do not run the original game. None of these results establishes
physical-GPU, native Metal or ARM64 validation. The GitHub transport commits preserve the tested source trees;
their metadata SHAs differ from the locally reviewed commits as documented in
the PRs. No original assets or private analysis databases are published.

## The nine work items

The following rows distinguish a delivered prerequisite from completion of the
whole subsystem. Subsequent PRs should update this table only after the actual
owner path, tests and sequential independent reviews are complete.

| Item | Current boundary | Next implementation and acceptance |
| --- | --- | --- |
| 1. Stabilize the original script stack | PRs #58–60 are merged and tested. Source clear and Process retain interrupted state and do not replay uncertain frees. | Keep these guards and real-bank fixtures when adding commands. Unknown commands must stop at their reached program counter. |
| 2. Shared world and persistent lighting | Persistent lighting is merged in #61; scene2 crash-to-canonical-model binding is merged in #62. | Preserve the same owner identity when extending other effect families. Before/after GPU and ownership-disposal tests cover this crash path, including stale generations and retained physical pieces. This does not complete every damage callback. |
| 3. Original-content loading frontiers | Empty model-cache duplication is fixed in #63; TrafficGenerator kind 18 source intake and teardown are implemented in #65; general companion loading, controller sharing and reached scale evaluation are implemented in #66. | Publish and consume the actual mission-owned animated-model list. Add genuine Bridge kind 20 construction, Init, shared data and teardown; handle later reached actor kinds. Preserve explicit GameInit/Tick and interrupted-cleanup boundaries, and rerun the same original-data inputs. |
| 4. Source-owned VM over the mission world | FINDFRAME/FINDACTOR execute through the actual `MissionContentOwner` composition in #64; #67 exposes this through the retained session and one validated Scene selection. | Connect the genuine constructed Game/lifecycle owners, then implement a reached command such as FRM_SETON against the canonical frame and its required Game effects. No second program, name registry, detached world or default owner during callbacks. |
| 5. MissionOpen, GameInit and Mission/Game Tick | Source loading and constructor ownership are implemented only in bounded phases. `PrefixComplete` is not GameInitialized. | Compose the original Open dependency order, roads/navigation, actual initialized state, Scene/Driver/model/game tick order and active Done/Close. Required unavailable services stop at their reached prefix. |
| 6. Differences and recorded cutscenes | Recorded input and the first actor-binding prefix exist. There is no complete recording playback path. | Apply differences to the retained world before lookup; bind all reached records/camera to actual actors and shared pools; implement timing, state transitions and restoration on Close. A successful load is not Playing. |
| 7. Player, profile/input, camera, HUD and menu | Profile/control tables and bounded player/camera helpers exist. The host menu's static intro and diagnostic chase camera do not satisfy original gameplay. | Route live profile/input values through the real selected player/vehicle control path; implement evidenced original camera modes/collision and HUD consumers. New Game and retry must use the actual mission lifecycle. |
| 8. Damage, AI, navigation, traffic and interactions | Actual car physics/damage and owned navigation intake provide dependencies. Detach/lamp/wheel callbacks and AI/pursuit remain reached frontiers. | Add each real damage effect to the same visual/collision/physical owners. Implement road/path queries, actual actor movement and traffic/pursuit in the original GameTick order. Loading a traffic actor does not implement spawning. |
| 9. Authored first-mission progression and persistence | Original objectives, failure/retry, completion and mission save/restore have not been demonstrated. | Execute the original scripts and their real conditions/events, objective/HUD effects and save/lifecycle dependencies. Validate menu → intro → MISE01 → authored completion, plus failure/retry and checkpoint restore. No replacement mission state machine. |

The implementation order follows dependencies, so work on a general loader or
owner can proceed while a later native-only callee is being reconstructed. Each
PR still has one author/owner, an immutable reviewed candidate, two sequential
independent final-diff reviews and exact execution receipts. There is no new CI
or parallel inspection world.

## Corrected reference map

The [2026-10-27 roadmap](parity-roadmap-main.md) describes an older reference and
retains its historical observations. At the new pin, the following distinctions
change the research queue:

| Area | What the pinned reference now contains | Remaining dependency |
| --- | --- | --- |
| Mission | `C_mission::Tick` and Open/Close source bodies. | Actual caller-owned fields, time-shift, sound, Scene, Driver, reverse model list, initialized Game and transparent-object effects. |
| Game | `C_game::Init`, `Done` and `DoneIntern` bodies. | Their real derived actor/resource callees; `C_game::Tick` still needs original instruction evidence. |
| Camera | Substantial `G_Camera::Tick` and mode/reset behavior. | Live player/vehicle/profile/collision and unresolved mode helpers; diagnostic camera behavior is separate. |
| Differences, roads and recording | Named entry points and some supporting behavior. | `LoadDiffData`, `Roads::LoadData` and several recording operations remain upstream stubs; use the original binary contracts. |
| Player controls | The entry named `UpdateControls` is a small car-control wrapper in the inspected PC binary. | It does not stand for the complete on-foot input/movement path. Trace the actual derived dispatch. |

Primary private REA receipts were captured for the relevant PC entries, including
MissionTick, GameTick, LoadDiffData, RoadsLoadData, recording operations, bridge and
traffic intake, model duplication, companion loading and interpolator evaluation.
Static recovery does not close runtime gates or add accepted-function credit.
PC and PS2 names/correspondences must retain their own evidence; a linker name is
not proof of identical layout or behavior.

## Original-data regression boundaries

These observations are from the fresh audit at
`1638ce36f57be39aa33787f1fcaab1ed328965a7`, before the continuation PRs above.
They are read-only/load-prefix observations, not results from the latest main.

| Input | Recorded boundary | General capability to check next |
| --- | --- | --- |
| `intro` | Scene2 prefix completes: 358 objects, 28 actors, 15 source programs; tracked terminal owners return to zero. | Production VM binding/effects, GameInit and the actual recording/lifecycle chain. |
| `mise01` | 3,183 objects and 38 actors, including two Car/Vehicle owners; stops at kind 20, cursor 3,814,295 of 3,840,614. | Genuine Bridge construction, Init and shared resource/LoadData behavior before later initialization. |
| `mise02-saliery` | 59 completed objects; duplicate of a valid zero-frame model stops at cursor 382,344. | General empty-hierarchy duplication and actual retained-child cleanup. |
| `mise03-morello` | 2,342 objects and 29 actors; stops at kind 18, cursor 3,934,923 of 3,988,486. | Genuine TrafficGenerator data/resource ownership and teardown. |
| `00menu` | 69 completed objects; companion-animation boundary at cursor 466,049. | The real loader/controller path, supported materials, and combined position/scale evaluation. |

The implicated empty model is a valid 19-byte 4DS with no frames or materials.
The menu companion contains a position channel and a scale channel. These are
regression inputs for general functions, never model-name special cases.
Future probe updates must record the exact tested source, new cursor/frontier
and actual cleanup outcome. Do not carry an old cleanup result onto a new path.

The #63 probe on tested source `954c332311a63be5cd940a29100d4feaecc48c06`
advances Salieri to 118 objects and cursor 592,342 on the same 1,425,974-byte
scene2 input. It stops at an animated model whose companion has a two-key
rotation track. This is not a successful whole-load result: terminal disposal
also stops at `Cache(Model): Interrupted`, retaining 93 frames, 39 cache
allocations and 6 sounds. The exact input hash, command, source tree and
validation scope are recorded in #63. Companion loading and safe handling of
retained interrupted cache owners remain necessary follow-ups.

The #65 probe on tested source `09b5ec12c55e9ff85c7c9fb9af2ac6d6562c1bea`
advances Morello from cursor 3,934,923 to 3,938,133 on the same 3,988,486-byte
scene2 input. Four actual TrafficGenerator owners retain 13, 14, 19 and 2
records, with all reported record and weight bytes known. The next frontier is
`UnsupportedKind(6)`; the diagnostic exits 1 for that remaining load boundary.
Terminal disposal reaches Complete and every reported lifetime counter returns
to zero. This verifies source intake and disposal; Traffic GameInit, Tick and
pedestrian spawning remain unimplemented by this change.

The #66 probes on tested source `6520e87340549d144ae99d4f96105356c3c1e2de`
load the actual companions for Salieri and the menu. They reach the next
animated-model list insertion dependency with 118 and 69 completed objects,
at cursors 592,342 and 466,049 respectively. These object counts/cursors are
unchanged from the preceding animation frontier; the completed work is inside
the reached model. ModelOpen returns zero and its cache operation completes.
Both diagnostics still exit 1 for the remaining mission list dependency, but
terminal disposal now reaches Complete and every reported lifetime counter
returns to zero. The #63 retained-owner result above is historical, not the
cleanup outcome of #66. Actual mission animation scheduling and rendered menu
animation are not established by these probes.

## Separate LS3DF and platform track

Keep full library parity separate from first-mission compatibility. Remaining
animation combinations, lighting/fog, shadows/projectors, mirrors, particles,
weather, audio and resource-lifetime behavior need their own update/draw/mix
integration and representative data. Native Linux hardware and macOS ARM64
Metal/audio remain distinct from software Vulkan. A portable numeric backend
must be compared with the evidenced x87 domain before replacing that backend;
host `f64`, libm or clamping are not implicit compatibility policies.

See [coverage](../../COVERAGE.md), [progress](../PROGRESS.md) and the
[Rust compatibility boundaries](../architecture/rust-compatibility-boundaries.md).
No coverage percentage or accepted-function count changes solely because this
plan, a static receipt or a PR exists.
