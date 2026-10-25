> Historical narrative from `docs/PROGRESS.md` at baseline `0da90fb`, before the texture/Scene/actor-loader publication. Original narrative wording is retained below; only relative link destinations are rebased for this archive location. Current status is in [README](../../README.md), [coverage](../../COVERAGE.md), [progress](../PROGRESS.md) and [goals](../GOALS.md).

# Progress

## Reference source audit — 2026-10-25

A fresh [three-project audit](../REFERENCE_PROJECTS.md#current-source-audit--2026-10-25)
observes mafia-re `109954d6` (17 commits beyond the last source observation),
unchanged MafiaUnity `0d7a12f0`, and OpenMF-Archived `7167e74d`. The new mafia-re
`main` supplies a startup/mission caller map; its mission and game-loop bodies
remain stubs. The immediate dependency is real flags-2 texture creation/cache/
Release for the Scene constructor, then shared weather lifetime and original
mission ownership. Managed `0x40` creation is a separate path.

This is source inspection only, with no upstream build, fresh REA/original
execution, new Rust implementation or rendered/interactive evidence. The
behavior pin `86a7b15c`, **311 entries / 114 contracts**, **0/12** gates and all
historical snapshots are unchanged.

## Scene weather and resource maintenance — 2026-10-25

Code commit `89308ce` adds `I3D_scene::WeatherSystemReset` and
`S_sector_sound_properties::SetupDefaults`, with actual PS2 symbol correspondence
and original PC instruction comparisons. Eight hundred direct inputs comprise
736 weather and 64 sound-default cases, including incoming precision P0/P1.
Full 65,536-byte fixture arenas and logical sticky P match actual Cargo debug and
release builds; native O0/O3 wrappers add no cases. Original fixed-angle math runs
without numeric mocks. Host CW/MXCSR equivalence remains unclaimed.

Loaded resources now expose separate vertex and index reclamation stages, and
standalone Car shutdown executes vertex before index over its actual owner.
Shared meshes and live vertex buffers survive maintenance. A synthetic CPU
backing-loss test checks poisoned-manager refusal without native failure credit.
The Scene helper admits only these two effects and rejects other boundaries.

The 265-file Source-v2 passes 1071 Rust tests, nine REA-wrapper and 47 progress
tests, formatting, locked all-target checking and Clippy. A fresh debug window
exercises three W driving runs, three R reloads and four zero-owned-resource
cleanups; five caller-owned Scene-prefix frames remain explicit.
[New screenshot](../screenshots/2026-10-25-resource-reload.webp).
Two sequential independent non-author reviews accepted the bounded source.

The accepted ledger is **311 entries / 114 contracts**, with
**23 static_inspection / 91 not_integrated**; strict verification remains **0/12**
and whole-function coverage N/A. Weather/default workers are not yet called by
the live Scene constructor. Viewport, shared Snow creation/lifetime, full Close,
original File Open/reload and mission ownership remain open. The authored MISE01
program probe still stops at FINDACTOR/UnsupportedWorldBackend, PC 3 / line 4;
the application does not yet execute its mission scripts.

## Previous standalone driving integration

Frozen Source4 composes the original-data Car lifecycle, static and dynamic
collision, material/tyre tables, crash-model replacement, physical `C_crash_obj`
debris and sound through canonical owners. Plain-glass geometry and fragment
lifetime workers are bound. Runtime requires original assets and does not open
an original executable or DLL. Full Game/player/mission ownership and whole-world
library parity remain open.

The full gate passes 1066 Rust tests (110 assets unit, seven assets integration,
910 engine library, eight engine integration, 26 application and five renderer),
nine REA-wrapper tests and 47 progress tests, plus formatting, locked all-target
checking and all-target Clippy. The 264-file source guard agrees before and after
the gate. Current Apple arm64 checking is blocked by missing CoreAudio SDK headers;
native macOS and Metal execution remain untested.

Three verbatim original-input trajectories pass in both actual opt0/opt3 profiles:
900 prop-collision steps, 900 glass-damage steps with chase-camera/listener/PCM
updates, and 1800 idle steps. One physical prop is created in the prop trace.
All six runs leave zero owned resources, with five separate caller-owned
Scene-prefix frames. Only damaged glass material is observed; exhausted-window
fragments and their sound remain unexercised.

Both windowed profiles pass 26 negative CLI cases, exact legacy taxi/police/MISE01
pixels, live W/S/A/D/Space controls, focus loss, R reload and two cleanups. Maximum
displacement is 12.224 metres in debug and 46.141 metres in release. The genuine
18-second release video records 55.038 metres maximum displacement, with 540
decoded frames and 330 distinct frames. It includes 17.44 seconds of actual
original-resource stereo audio at 48 kHz. Software capture and simulation timing
diverge by up to 0.706 seconds; audio is neither padded nor stretched. Original
renderer/audio parity, perfect synchronization and hardware audio output remain
unverified. Both sequential independent non-author reviews approved the bounded
Source4 milestone with no open findings, bound to code/docs/media commit `78c130e`.

The sealed native comparison passes all admitted current-source Cargo opt0/opt3
cases under 135 matching 264-file source guards. Its count categories remain
separate: 480 baseline named inputs / 377 semantic capsules and five negative
controls; 631 new named method/kernel inputs including projections and precision
controls; four genuine library-owner queries; and four new availability-negative
controls. These categories have no combined positive or unique total, and profiles
add no duplicate credit. Controlled world/resource boundaries, CPU kernels and
typed projections retain their individual scopes. Original code runs under masked
CW007f PC24/RN; normalized Rust results/effects and its modeled logical sticky P
are compared. Rust/native host hardware CW/MXCSR equivalence and other
exception/condition bits are unclaimed.
Whole Game, complete Scene, GPU and whole-function parity remain unclaimed.

This integration adds zero accepted entries or contracts: **309 / 113**, with
**23 static_inspection / 90 not_integrated** and strict **0/12** unchanged.
The authored MISE01 VM still stops at FINDACTOR/UnsupportedWorldBackend on
PC 3 / source line 4. The accepted `bf8b170` prerequisite baseline and all dated
snapshots retain their original scopes.

## Previous bounded prerequisite milestone

At `bf8b170`, the accepted scope is **309 entries / 113 reviewed contracts**, **23 static_inspection / 90 not_integrated**, strict **0/12**. Whole-function coverage remains N/A.

Original car level tables now load from the user's DTA data through the reconstructed loader and packed-parameter converter. Eight distinct model/allocator configurations pass in both profiles (16 executions), including repeat-load rejection and explicit cleanup with zero tracked Car/Game/model resources. Five host Scene-prefix frames remain explicit context. Runtime needs original assets without opening an original executable or DLL.

The new bounded contracts cover Car/Actor/Vehicle GameInit and cleanup, Engine/Move/Tick scheduling, wheel publication, contacts and static sphere/box queries. Original-instruction comparisons retain declared raw entry state, original numeric order and explicit world/resource boundaries. Internal Move phases, alternate APIs and build profiles add no duplicate entry credit. Required external backends remain incomplete; full application GameInit, body simulation, driving and mission execution remain unavailable.

The car inspection mode now previews recovered steering on the same initialized wheel frames. Hold A or D to turn the wheels; arrows and W/S control the camera, and R explicitly reloads the Car and resets input. The car body stays stationary.

Actual authored MISE01 remains FINDACTOR/UnsupportedWorldBackend at PC 3 / source line 4; no later line executes. All six first-mission gates remain open, with strict verification 0/12.

Historical Car construction/runtime milestone at `5b28728` retains 272 entries / 93 contracts: 34 native constructor completions plus four separate prefixes, and 96 separate constructor/primary-Init/destructor inputs. Eleven original models share the initialized CPU owner and explicit cleanup. Its allocator, native stack, Scene context and lack of GameInit/driving integration remain immutable in the dated contracts and observations.

Historical bounded Car Init milestone at `df50d2f` retains 271 entries / 92 contracts: 14 AL1, 18 AL0 and 60 host-prefix main inputs, separate helper comparisons and eleven-original-model CPU lifecycle composition. Its native-constructor intake and lack of new rendering remain immutable in the dated scope and observation; the current constructor/runtime milestone adds separate source-bound evidence.

Historical same-arena Vehicle milestone at `75e380d` retains 264 entries / 85 contracts: bounded Vehicle Init and explicit destruction, flags 0/1 parameters, AXIS24/Setup12/point/control records and six-original-car CPU composition. Its original C_car prefix frontier and controlled outer effects remain immutable in dated scopes and observations; the current Car milestone supersedes that frontier only within its new bounded context.

Historical shared Scene/Sector milestone at `43e04ca` retains 257 entries / 79 contracts, with 372 unique bounded AddFrame/Close inputs: 321 normal completions and 51 separate controlled host-effect prefixes. Three original models with 92 source frames composed with one live ModelArena per fixture, two ownership-prefix scenes and eight retained sectors. Constructor, resource/driver callbacks, loaded-visual destruction and native raw-heap limits remain immutable in its source-bound contract and observation.

Historical vehicle-component milestone at `21c555b` retains 255 entries / 78 contracts: 1,159 unique record-helper inputs (1,153 normal completions and six fault prefixes), 311 bounded Init-tail inputs and 591 earlier vehicle regressions. Original-data taxi composition preserved 67 frames and stopped before wheel enumeration under explicit constructor/frame/allocator controls. Its source-bound contracts, postprefix opaque-stack limits, failure-prefix policy and lack of complete Init/driving acceptance remain immutable.

Historical BODYCOL/fast-tree milestone at `494ede8` retains 245 entries / 77 contracts: 478 record observations (477 completions and one fault prefix), 164 physical BODYCOL cases and 31 fast-tree/loader cases with 1,436 ordered operations. Its source-bound controlled effects and ownership limits remain immutable. The resolved-slot bridge retained 119,019 records / 348,677 references and 2,044 rays: 288 hits, 575 misses and 1,181 explicit errors, without original scene construction or whole-map native parity.

Historical component/volume/sound milestone at `1026880` retains 227 entries / 74 contracts: 7,304 of 7,371 direct-volume matches (67 exclusions), 2,129 mixed-query matches, 4,031 physical sound/shift inputs and 188 new Init inputs. Direct/mixed volume physical and QEMU outputs agree; mixed-query fraction remains independently unverified. Original audio methods and BODYCOL passes were explicit boundaries at that freeze. Its immutable contracts and observations retain those limits.

Historical Init/collision milestone at `5eb220a` retains 222 entries / 72 contracts: 239 Init-prefix matches, 293 of 322 preparation matches (26 numeric and three binding exclusions), and 2,845 of 2,869 static-query matches. Physical classifier evidence resolves seven QEMU threshold discrepancies; partial-loader degeneracy remains controlled QEMU evidence. Its immutable scopes and earlier observations retain their accepted source and limits.

Historical vehicle-world milestone at `46db9f5` retains 219 entries / 69 contracts: 1,024 constructor configurations, 32 separate constructor-to-controls trajectories and 5,215 admitted drivetrain configurations with 52 numeric exclusions. Its 79-member KLZ intake and controlled loader/geometry-binding evidence retain their original limits.

Historical driving-prerequisite milestone at `38bd2c4` reached 214/67 with original parser/398-row recognition, five detached input setters and the triangle-only collision leaf. Its static GPU/UI and placement evidence remains historical. Earlier source, frame, actor and resource observations retain their original limits.

Historical frame milestone at `4c42068` reached 195/63 with 151 admitted matches from 474 original-instruction inputs and 166 separate Rust cases. Its Target/original-name search, shared user-data records and bounded release retain their accepted domains. Actual Tommy targetN was exercised in Rust; no actual targetE or successful original-model Close was claimed.

Historical actor-world milestone at `d5c6666` reached 192/62 with 88 admitted matches from 93 original-instruction inputs and 140 separate Rust probes. Its base-actor/source ownership remains accepted. The frame virtuals were controlled dependencies at that freeze; the current model adapter now supplies that bounded path. Allocator internals, derived actors and mission-world wiring remain open.

Historical lookup milestone at `da19285`: two canonical LS3DF entries reached 181/61. Its 207 unique original-instruction inputs, 13,248 normalized bytes per profile (168 zero padding), six MessageBeep callbacks and 35 native-only secondary checkpoints remain accepted within their original scope. Separate 108 Rust probes and 168 original-resource checks remain Rust evidence; the three attempted original-model closes stop at existing unsupported derived destruction. The current mission frontier is unchanged.

Historical declaration milestone at `251e05b`: DIM_ACT/DIM_FRM added zero entries or contracts, retaining 179/60. Its 460 original-instruction inputs / 492 dispatches, 618,240 normalized bytes per profile and 55 separate Rust probes remain accepted evidence; allocation/free bodies were intercepted and native allocation failures excluded. Its unsupported-command line-4 frontier is superseded by the recognized-command, missing-world-backend frontier above.

Historical replay milestone at `ceb988a`: two busy/setter entries and the live `RECWAITFOREND` bridge reached 179 entries / 60 contracts. Its 1,161 original-instruction QEMU cases (345 busy-leaf, 708 setter, 102 bounded-dispatcher and six scheduler) and 2,972,160 normalized output bytes per profile remain accepted prior evidence, counted once across debug/release. Unused zeroed capacity is included; full native memory/guards are excluded. Controlled callbacks cover replacement, vector mutation and reentry. The scheduler fixture changes its global after dispatch without running original replay Update. Its separate 76-probe receipt and line-2 declaration frontier are historical; the current frontier is line 4.

Historical Installation milestone: At `b856d76`, existing `Installation::read` archive callers now use `NativeFiles` with the selected original name, table row and archive owner. First-archive precedence remains A8, A2, A6, A1, AC, AA; read errors propagate without retry or fallback. Menu and static scene loading exercise this route. Independent checks match all bytes of 49 original resources / 26,228,595 bytes per debug/release profile against the frozen Rust baseline `1a1a8df`; this is Rust–Rust evidence. Ten rendered/keyboard-interaction captures match that exact baseline with zero changed pixels. Counts remain **177 entries / 59 contracts**, strict **0/12**: no new canonical entries, contract credit or original-runtime comparisons. Audio playback and the first mission remain unimplemented. The selected app path composes Create/constructor, unlock, archive open, archive Read and Close; it bypasses the `dtaOpen` wrapper and provides no AddRef/Release/destructor, Seek or app WAV-use evidence. Selected non-ASCII archive names fail explicitly; loose Unicode remains supported. Checked empty entries are an explicit host accommodation, not native empty-open equivalence.

Checks for that historical source freeze: 502 Rust tests (104 assets unit, seven assets integration, 365 engine library, 21 application, five renderer), nine REA-wrapper tests and 47 progress tests pass, plus formatting, locked workspace checking and Clippy. Independent synthetic probes pass 45 assertions per debug/release profile. All six actual archives open; five supply selected resources, with AA shadowed by A8. The ten equal images comprise three headless MENU/INTRO/MISE01 captures, three 360-frame windowed runs and four keyboard states: menu → New Game → static INTRO → Escape → static MISE01 → Escape → menu. These are static inspection, not an authored cutscene or playable mission. Apple arm64 all-target checking passes for the exact five-file source freeze; native macOS linking and Metal execution remain untested. Two sequential independent reviews accepted the exact final source. No fresh REA analysis or original-runtime invocation was performed in this batch.

At that freeze existing contract rows remained **20 static_inspection / 39 not_integrated**: their combined domains cannot represent this selected archive subset faithfully. The 25-binary catalog, scoped history and generated SVG were unchanged. Replay state/VM work and production texture-worker/surface ownership were still pending.

Historical filesystem milestone: At `f04cc43`, read-only `FilesystemEffects`, `NativeFiles` and `NativeStream` connect the reconstructed DTA lifecycle to real files through a library API. Checked retained-file indexes, original path separators and unlock-key conversion compose with snapshot/cache guards, selected-entry extents, quotas and stream-generation checks. Thirty host test groups and 16 separate malformed-admission cases exercise this host boundary. Rust debug/release replay through `Handles<FilesystemEffects>` matches the previously sealed 202 native Read observations; this adds **zero original executions** and does not establish native equivalence of facade preflight. A separate Rust–Rust comparison checks 13 assets / 1,618,556 bytes across six installation archives: all 13 match direct archive reads, and 12 also match `Installation`; the remaining AA entry is intentionally shadowed by A8 there. Counts remain **177 entries / 59 contracts**, strict **0/12**. Application `Installation` and texture-worker routing remain unchanged; audio playback, first-mission execution and new rendered/interactive validation remain open.

Historical WAV milestone: At `f89f542`, bounded PCM16 mono/stereo WAV archive decoding composes with Open/Read/Seek/Close: **177 scoped entries / 59 contracts**. Native comparisons cover 202 Read observations (172 nonfailure, 30 released-handle failures) across 26 archive fixtures plus four supplemental trajectories reusing four fixtures, with 66 explicit Seek context rows. The 510 normalized log rows and 7,280,960 complete-buffer bytes include guards and unrequested regions; debug/release count once. Separately, 128 logical decoder vectors and 18 scripted effects plus four direct-buffer effects check Rust behavior, without native failure injection. Eight fresh REA results support the codec; identical PC/PS2 table constants and PS2 linker names add no PS2 runtime claim. Scope is bounded 16-bit mono/stereo; generic non16 LZ, other channels, Write and full DLL parity remain open. Application/audio playback and first-mission integration remain open; no new rendered or interactive validation.



Updated 2026-10-23 UTC. The Rust engine supports original-data model inspection and bounded sequential skeletal animation through owned controller, clock, dispatch, Target, morph and CPU skin paths. Bounded model/dummy/unbound-Joint/Target lifetime preserves external survivors and rejects stale playback after parsed-source replacement. Independent material, face-group and mesh-level primitives now have a ten-slot mesh owner with shared references and bounded volume refresh. A standalone managed visual-owner bridge now connects shared mesh/material ownership to actual bounded IB/VB allocation and cleanup. A separate per-object layer now owns ten unique plain LODs and protects shared mesh references. ITexture/C_str lifetime now includes finite managed record/queue/pool removal and Close composition from seeded state. Actual bounded managed-record creation now composes that chain. Bounded Request queue production retains its existing creation/removal and Close composition. Standalone pool unload composes Request/removal from seeded pool state; Close is covered by a separate Rust unit. Bounded standalone managed texture pool release now retires admitted pool state: it clears loaded bits/weak owners under one lock, then releases each pool’s optional system and reverse-visited slot resources, frees its array and resets headers/memory while preserving grow. ReleaseManagedTextures is a pinned mafia-re working name without recovered original PC/PS2 symbol. Full Create/Restore, worker/upload, whole-device shutdown, derived lit/skin/morph LODs and loaded-resource/GPU adapters remain open. Original models and clips remain regression evidence; texture/GPU backends, filename, loaded visual-resource and scene lifetime remain open. The original Czech menu, static INTRO and bounded MISE01 neighborhood remain available. Cinematic and first-mission execution are unimplemented.

The generated table records historical scopes at their accepted commits. The replay row's line-2 probe predates the current line-4 frontier above. Declaration slots add no count or snapshot; earlier standalone comparisons retain their original receipts.

| Measure | Verified status and limits |
| --- | --- |
| Linux workspace | 537 Rust tests (104 assets unit, seven assets integration, 400 engine library, 21 application, five renderer), nine REA-wrapper tests and 47 progress tests pass, plus formatting, locked checking and Clippy. |
| Standalone managed texture Restore | Six admitted cases compare 5,565 ordered effect rows and 1,109 state rows once across debug/release. They span eight Restore and ten existing Release calls, including six effect-only cleanup phases; existing Release receives no duplicate credit. No return/status or Rust instruction/store comparison is made. The saved native-only capture includes seven cases, nine Restore calls, 87,496 instruction steps, 7,410 stores and 1,024 copies of 44 bytes. Native case six has fully backed registry count 4,097/capacity 4,096 but is excluded by host coherence. Final comparison reused saved native captures without native reexecution; Rust debug/release used 12 hash-bound copied production modules. The required host owner flag 0x40 is unused by these operations and changes no expected projection. Valid controlled COM outputs, including failing HRESULTs, are exercised; no real GPU or COM implementation is established. Coherent extents, live identities, separately owned alias references, representable lengths, nonreentrant backends and bounded terminating reconstruction constrain admission. |
| Earlier standalone managed texture pool release | Three seeded scenarios compare six release calls: six state rows, six synthetic void return-class rows and 108 ordered effects (12 lock callbacks, 78 controlled COM Release callbacks and 18 allocator calls), counted once across debug/release. Native EAX is unspecified. Controlled COM identities start with 100 references; actual Direct3D destruction is untested. Native fixture Create is context only, with no Rust Create comparison. Native-only tracing observes 1,404 instructions and 106 stores with surrounding-byte preservation; Rust intermediate store observer parity is excluded. Nonnegative bounded extents, live owners, valid slot COM identities and allocator ownership, sufficient alias lifetimes and nonreentrant/nonconcurrent backends bound admission. |
| Earlier standalone managed texture pool unload | Three cases compare five unload calls plus one existing Request and one removal call: seven state rows, seven return rows and 17 ordered effects in debug/release, counted once. Rust seeds pre-Unload state; native creation/pre-Unload loaded Request remain context only. Native-only tracing observes 439 instructions and 16 stores with surrounding-byte preservation. Borrowed graphics raw-byte evidence is native-only; a separate Rust unit verifies retained borrow. No internal store observer, whole SceneClose or application parity claim. |
| Earlier managed texture Request producer | Eight admitted cases compare ten Request calls plus three existing removal/Close calls: 13 return rows, 13 state rows and 41 ordered effects in debug/release, counted once. Actual creation precedes Request in three cases; other managed records are seeded. Selected original record stores compare to Rust storage effects; queue intermediate stores remain native instruction evidence. Four native-only calls, host guard rejections, worker/upload, full Create, pool shutdown and GPU/application integration receive no new parity credit. |
| Earlier actual managed-record creation | 5 admitted cases / 55 phases compare record fields and derived filename bytes, actual filename rejection, texture publication and registry append/growth plus existing cleanup. Effects distinguish 114 external calls and 98 selected storage-write/copy events. Original publication stores are native-only evidence; Rust internal callback alias/timing is excluded. Host preparation/identity/arithmetic/error bounds remain explicit. No full Create, Request/worker/upload, pool shutdown, GPU or loaded/application integration. |
| Earlier managed texture removal | Six admitted direct cases / 24 phases and one composed case / 28 phases compare 28,638 projection cells and 193 required effects. This includes 4,740 opaque conservation words and 104 out/put preservation cells, without new member-layout credit. Active record order, queue/pool owners, stale raw fields and later Close clears agree; pool payload survives and texture graphics Release is zero. Two native-only guard cases/four phases and original-only instruction tracing are outside admitted parity. No worker, creation/upload, pool shutdown, GPU or loaded/application integration. |
| Earlier nonmanaged texture/string lifetime | Nine direct cases / 54 phases and one composed resource case / 28 phases compare 7,084 applicable projection cells and 126 required effects. Positive refs, constructor field availability, owned aliases, repeat Close and admitted cache order compose actual object/mesh/material/IB/VB cleanup. Managed state, duplicate cache intake, native creation/upload, high-word/padding, native selfdelete/pointer ABI and loaded/application/GPU integration remain excluded. Allocation-null and backend-error paths are host/static evidence only. |
| Per-object plain LOD resources | 8 original-call trajectories cover 73 phases. Distinct generated LOD leases share the manager world with shared mesh levels. Ten ascending cleanups precede one whole-array clear and shared mesh Release, with raw mesh field retained. Constructor padding/vtable, primitive allocator/pointer-return parity and lower visual/frame cleanup are excluded. Fresh attachment/allocation are host adapters; no SetMesh/Create/upload, derived LOD, loaded or application integration. |
| Earlier standalone visual-owner bridge | Four composed original-call trajectories cover 81 phases through actual primitive and IB/VB manager bodies with nonnull descriptors, matching 10,095 applicable normalized cells and 100 required effects. The 26 direct numeric returns, 13 fixture-admission booleans and 42 synthetic statuses remain distinct. Stable leases preserve shared backing identity, block cleanup and distinct IB/VB reclamation. Controlled dependencies and fixture intake are separate. Native creation/Lock/upload, callback-time embedded descriptor publication, loaded-resource/per-object LOD adapters, texture/GPU backends and application integration are excluded. Zero new canonical entries or scoped contracts. |
| Earlier buffer managers | Fourteen complete original-call trajectories cover 85 phases, 1,062 normalized state rows and 352 ordered required boundary effects. A separate five-phase prefix adds 82 rows and 20 effects before a host insertion guard; native continuation is excluded. Returns distinguish 49 AL booleans and one full-width base-or-minus-one result from 40 synthetic statuses. Controlled allocator/COM/driver/diagnostic boundaries are not backend implementations. No visual-owner bridge or loaded/application integration. |
| Earlier mesh-object ownership | Ten persistent original-call scenarios cover 142 phases, 166 mesh and 360 dependency snapshots; 79 genuine numeric returns are distinct from 63 synthetic statuses. Eighteen original-triggered IB/VB callbacks use null descriptors; actual manager implementations are excluded. Separate bounds evidence matches 83 admitted raw-bit cases; six wider inputs are host rejections, not native behavior matches. Hidden higher-slot references survive truncation and final prefix release. No loaded-frame or application integration. |
| Earlier resource primitives | Six persistent original-call scenarios cover 43 phases and 172 normalized snapshots: 1,260 applicable original-resource cells, including identity/kind/liveness, plus 254 separate external fixture cells. Twenty original-triggered dependency effects match; eight fixture-only texture releases are separate. Required controlled texture/buffer boundaries, positive references and unique chains/blocks bound the scope. Native backend implementations, allocator/pointer-return parity, mesh-object/LOD and loaded frame/model integration are excluded. |
| Earlier frame/model lifecycle | Ten persistent original-call scenarios cover 104 phases and 313 snapshots, matching 3,139 applicable state cells, 163 registry cells and 126 Target identity/count/order cells. The 34 original numeric returns are distinct from 70 synthetic replay statuses. Controlled external dependencies, dead-state normalization and unbound-Joint scope limit the claim; filenames, controller teardown, allocator events, native last-destroyed bookkeeping, associated Joint-slot differential coverage and loaded visual/scene destruction remain open. |
| Controller ownership | Eight PC slots (PS2 has three) retain source identity, binding order, evaluator allocation/cleanup and native partial-failure prefixes through explicit resource backends. Native and Rust debug/optimized projections match 2,908 control words across 27 snapshots; eight supplemental cases match 48 words for masked x87 NaN quieting, preserving getter storage. Inline construction, detached-allocation host reclamation, native allocator and full destruction receive no extra function credit. |
| Sequential composition | One live slot at unit weight drives exact-name position/rotation evaluation and persistent hierarchy/Target/morph/skin state. Across 206 operations, 1,413,160 native words match, including stop/resume, restart and stop/loop boundaries. Bounds and CPU deformation are explicit caller requests; no original renderer scheduling or full gameplay lifecycle is claimed. |
| Bounds cache | Loaded bindings use the recovered change-counter gate. A 206-record native probe covers 203 changed and three unchanged entries and a second unchanged query per record. The earlier missing-bounds-query comparison remains preserved: other state matches while 2,060 bounds words stay stale. An exploratory eager calculation likewise does not prove renderer scheduling. |
| Rotations | Checked linear/spherical Slerp matches all 22,103 admitted leaf cases; 1,947 of 24,050 inputs exceed the 1.5-radian spherical domain. Actual rotation tracks match 99,579 evaluator samples. Ambiguous rounding and other unavailable numeric domains fail explicitly. Existing Slerp/evaluator/normalization entries are not counted twice. |
| Game animation lookup | GetTrack searches existing 100-byte PC records in reverse with a required original-locale comparison backend. It matches 3,201 scripted-boundary cases plus 66,025 cases using unchanged original C-locale CRT code. PS2 supplies the original name, but uses stride 96/name offset 48 versus PC stride 100/name offset 0 and different guards. CRT, resource loading and animation-machine construction receive no function credit. Earlier animation-index sort/cleanup evidence remains retained. |
| Original assets | AC joins A8/A2/A6/A1/AA in the supported original-order subset. 147 models yield 421 exact skin/morph/Target suffixes totaling 2,172,046 bytes. Aggregate metadata quotas remain enforced. Other archive codecs/checksums and native resource ownership remain open. |
| Regression fixtures | TommyTAXIDRIVER static plus five independent breath01a poses retain 81,216 exact native words. The persistent sequential comparison uses the same original source pair. A separate Tommy.4ds binding/evaluator probe retains 104,820 outputs; its bytes and one named-frame index differ. These fixtures do not establish coverage of every original asset. |
| Rendered output | The current provider preserves ten exact-baseline MENU/INTRO/MISE01 and keyboard-transition images with zero changed pixels. Earlier sequential captures, a 400-frame X11 run, a 360-step animation regression, static/sample poses, civil02 and position-animation checks remain separate historical evidence. |
| Static binary analysis | Game 4,784/4,914, LS3DF 1,555/1,714 and rw_data 176/234 retain raw denominators including external rows. Recognized in-image cohorts have complete output while exhaustive discovery remains open. The isolated derived REA profile retains Game ab910 pseudocode with 13 warnings and unchanged body/inventory; the original failure remains in history. |
| Pinned library correspondence | 764/1,255 pinned upstream LS3DF addresses have exact inventory correspondence; 491 remain unresolved. This is not function implementation parity. |
| macOS / Metal | Apple arm64 all-target checking passes for the accepted five-file lookup source. Native macOS linking and Metal execution remain untested. |
| Campaign | Weighted multi-slot evaluation, scale/notes, pose-set/callback lifecycle, actor/resource ownership, collision, driving, combat, police and mission execution remain open. The strict full-system gate remains 0/12. |

The previous Restore snapshot added bounded standalone Restore at 0x7a710 in one contract. Bounded standalone managed texture Restore now reconstructs admitted system/default pools and reserves the registry through controlled device/storage effects. It composes the accepted Release dependency, selects raw format once, uses returned descriptor Size with signed budgets and wrapping memory arithmetic, grows arrays in 1,024-slot steps and preserves registry entries. RestoreManagedTextures is a pinned mafia-re working name without recovered original PC/PS2 symbol. That snapshot preserved all 54 earlier scoped rows, histories and supplementary observations. Six admitted cases compare 5,565 ordered effect rows and 1,109 state rows once across debug/release. They span eight Restore and ten existing Release calls, including six effect-only cleanup phases; existing Release receives no duplicate credit. No return/status or Rust instruction/store comparison is made. The saved native-only capture includes seven cases, nine Restore calls, 87,496 instruction steps, 7,410 stores and 1,024 copies of 44 bytes. Native case six has fully backed registry count 4,097/capacity 4,096 but is excluded by host coherence. Final comparison reused saved native captures without native reexecution; Rust debug/release used 12 hash-bound copied production modules. The required host owner flag 0x40 is unused by these operations and changes no expected projection. Valid controlled COM outputs, including failing HRESULTs, are exercised; no real GPU or COM implementation is established. Coherent extents, live identities, separately owned alias references, representable lengths, nonreentrant backends and bounded terminating reconstruction constrain admission. Full texture Create, worker/upload, real GPU/device implementation, whole-device shutdown, derived LODs, loaded adapters and scene/application integration remain open; no new rendered or interactive validation is claimed. Static inventories, unknown whole-function counts, prior graphics, strict 0/12 and all six first-mission gates remain unchanged.
