# Source-based remediation stages

## Current merged validation — 2026-10-26

Thirteen implementation PRs are merged. This final integration record covers
the stages and their interaction; it does not add another implementation or
claim that the final record PR has already merged. Accepted production code is
`a4dee6d4cce02564e72d0fc23e6d9fe6fdc39abe`, Git tree
`0244444e9bc30b91fb7b516568c1fd8884cbd5ee`. The 324-file code/config fingerprint
is `66fc7ae72d5ff4214f40fc6f09305c1f9c04edae73659c7c5114bb4e8aafc61a`;
this record changes documentation/media only.

| Current check | Actual result |
| --- | --- |
| Required `scripts/test.sh` | PASS: format, all-target check, Clippy, 1310 Rust tests, 9 Python boundary tests, 47 progress tests and generated check. |
| Release workspace tests | 1310 PASS: 151 assets, 8 installation, 1102 engine library, 34 main binary, 9 program integration, 6 renderer. Debug has the same split. |
| Unchanged high-FIP x87 tests | Both PASS in debug and release, without command-line exclusions or source changes. |
| Opt-in GPU checks | 7 PASS on Mesa llvmpipe Vulkan: 3 renderer, 3 engine consumers, 1 binary refresh. The renderer command totals 9 with its 6 ordinary tests. |
| Actual original-data CPU probes | 1415 frames, 5 sectors, 14 portals, 83 hull vertices, 60 faces and 36 planes; 43 scene2 records, then the same `OBJ_` → `LMAP` replacement frontier at cursor 8013. All tracked keys/counters released. |
| Fresh standalone UI | W/A/D/S/Space, three R reloads and Escape; four cleanups with seven zero CPU counters, task-owned processes reaped/absent, new root-inspected screenshot. |

The actual-data world releases 1431 objects, 1401 meshes, 1416 levels, 4020
groups and 3972 materials. It contains **zero Lit objects and zero Lit LODs**;
new Lit destruction has separate synthetic/GPU tests and was not exercised by
this input. Backend counters were already zero before extra reclaim. These
safe Rust-only probes do not execute the original game or prove native
SceneOpen/MissionOpen equivalence. UI evidence covers standalone driving only;
PCM mixing was active, but host audio hardware was unavailable. No new video.

Per-stage and original combined-source sequential AI review records are retained.
Accepted production differs from reviewed combined code
`0a9d79dfae1bc949909f81e95cc80bc208a1d6bc` only in the separately reviewed
Stage 7c hard-link fix and its documentation. Two new sequential independent
reviews approve that fix; this is not a claim of two fresh reviews of all
324 final source files. Every implementation merge tree matched its tested
prefix plus that approved repair where applicable. AI records are not human
approvals.

Current verification receipt: `PR-MERGE-20261026-VERIFICATION-v1`, SHA-256
`e127df40d84b05c352041fc7a89dfa1c2dbad32245c54a0f85937384048a72a0`.
Source snapshot SHA-256:
`f60d8d13047b10ed3dcb5d9bd59b3d451d960bc4573c67d53fbf0343099694be`.
Source-acceptance receipt SHA-256:
`24dfd2343ab2155d246f88c737d1d1df31d94376f763d4be6c97ee942c9ec781`.

This source-based milestone adds no native-function, ledger or full-system
credit: accepted scoped work remains 311 entries / 114 contracts, strict 0/12,
and all six first-mission gates remain open. Saved lightmaps and the scene
`LMAP` payload association remain open; Stage 8a supplies the unconstructed Lit
branch only. The campaign remains unplayable.

The historical combined integration and publication sections below retain the
earlier draft branch, excluded-x87 run and publishing details verbatim. Those
results describe that earlier environment and are not the current merge gate.
The current final validation above supplies the accepted source/tree binding.


This register tracks the changes requested after the comparison with
`Marvisak/mafia-re`. It distinguishes implemented behavior from parser-only
support, connected presentation behavior, and dependencies that remain open.
An implemented source branch is not a claim that its whole subsystem is done.

Reference source: [`Marvisak/mafia-re` at
29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c](https://github.com/Marvisak/mafia-re/tree/29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c).
Starting project revision: `aa3da350991634f7ab1f5636585e8637f1316b1b` on
`rust-engine`. This work uses only the reference code as its behavioral source.
It adds no binary reconstruction, function/progress or original-runtime credit.

## Stage list and historical draft publication

The draft status below records the original publication. Current merged status
and validation are recorded above.

All linked changes are separate draft pull requests with incremental commits.
Each implementation has two sequential independent AI review rounds; the author
does not review their own implementation. The PR review records identify the
reviewed revision, findings, tests and remaining boundaries. These records are
not human approvals.

| Stage | Pull request | Implemented behavior |
| --- | --- | --- |
| 1 | [#3: 4DS compatibility](https://github.com/dnesdan/lost-heaven-reborn/pull/3) | Exact light/animated-material payload intake, UV-only LOD retention, original BMP/TGA path admission, connected scene TGA decode, KEEP_PIXELS storage semantics. |
| 2 | [#2: original-data loaders](https://github.com/dnesdan/lost-heaven-reborn/pull/2) | Owned effects trees, ordered program replacements from real patch streams, and all 576 animation catalog pairs with their sentinel. |
| 4a | [#6: matrix Make](https://github.com/dnesdan/lost-heaven-reborn/pull/6) | Missing-axis cross products, fallback and recursive scale handling, connected to existing frame transform consumers. |
| 5a | [#1: render resources](https://github.com/dnesdan/lost-heaven-reborn/pull/1) | GPU texture sharing across draw replacement, generation-safe pixel updates and reusable depth attachments. |
| 5b | [#9: live materials](https://github.com/dnesdan/lost-heaven-reborn/pull/9) | Actual live-arena material parameters, alpha tests, color/alpha blending, culling and draw ordering, with GPU pixel regressions. |
| 5c | [#12: live LOD and fade](https://github.com/dnesdan/lost-heaven-reborn/pull/12) | Camera-selected owned geometry, source distance fade, actual retained buffer-layout checks and redraw/resize replacement through cull and re-entry. |
| 5d | [#11: authored view orientation](https://github.com/dnesdan/lost-heaven-reborn/pull/11) | Retain quaternion-derived up as well as forward in the actual host metadata view, preserving roll and valid vertical-forward cameras. |
| 6 | [#4: sound lifecycle](https://github.com/dnesdan/lost-heaven-reborn/pull/4) | Playback states, source replacement order, live inherited scale and listener sectors, guarded derived ranges and cone normalization. |
| 7a | [#5: original image formats](https://github.com/dnesdan/lost-heaven-reborn/pull/5) | Original 16/32-bit BI_RGB BMP and indexed TGA branches, connected to source BPP/palette metadata and existing texture creation. |
| 7b | [#7: archive fallback](https://github.com/dnesdan/lost-heaven-reborn/pull/7) | WAV general-decompression fallback through real plain/encrypted archive reads and seeks, with selected-source failures preserved. |
| 7c | [#10: output writing](https://github.com/dnesdan/lost-heaven-reborn/pull/10) | Shared write-open/write/seek/close behavior, real bounded output files, partial-write discard and logical-output reservation. |
| 7d | [#8: menu language](https://github.com/dnesdan/lost-heaven-reborn/pull/8) | Version-selected raw TextDB bytes, glyph map and actual font family/metrics in the live menu. |
| 8a | [#13: unconstructed Lit objects](https://github.com/dnesdan/lost-heaven-reborn/pull/13) | Distinct LMAP ownership, six boxed constructor lighting records, private checked Lit geometry, view-aware LOD without ordinary fade, and the cyan constructor draw in live/static consumers. |

## Coverage and remaining work

The numbers below identify workstreams in this register, not function counts
or progress credits. "Implemented" means the bounded behavior described in its
stage document and tests. "Partial" identifies a real remaining producer,
consumer or ownership dependency; it must not be silently treated as complete.

| # | Workstream | Current result and next acceptance boundary |
| --- | --- | --- |
| 1 | 4DS light suffix | Stage 1 retains the exact 40-byte payload and hands it to the existing constructor/LoadImage path. Raw fields are not assigned invented meanings. |
| 2 | Animated-material record layout | Stage 1 consumes the two bytes and four signed 32-bit fields in the source order. Animated texture ownership/playback is still separate. |
| 3 | UV-only LOD records | Stage 1 retains the `0xffff` sentinel case, LOD0-derived UV count and independent group indices. Runtime materialization still rejects this branch before owned mesh-resource allocation; it needs the source texture-LOD/UV-switch owner. |
| 4 | Model texture paths and TGA | Stage 1 admits bounded safe BMP/TGA paths and verifies actual SceneBuilder decode, dimensions, fallback and quotas. Native non-ASCII filename behavior remains workstream 30. |
| 5 | KEEP_PIXELS | Stage 1 treats `0x8000` as texture storage metadata. It is not an unsupported shader effect. Keeping pixels does not implement additional material passes. |
| 6 | Effects data | Stage 2 connects real streams, owned trees, defaults and raw fields to the existing mission constructor. Full game startup still needs a genuine production mission allocator and game owner. |
| 7 | Program patches | Stage 2 connects nullable strings, signed lengths, per-file XOR index reset and ordered replacements to the existing program consumer. It does not invent missing script opcodes or mission execution. |
| 8 | Lightmap objects | Stage 8a implements the unconstructed subtype, constructor lighting records, private Lit LOD geometry and cyan draw. Full saved lightmaps still need scene-to-object payload dispatch, constructed lighting and LightMap ownership, rectangle packing/postprocessing, second UV/texture and the actual constructed-lightmap draw. A plain unlit fallback or standalone parser is not completion. |
| 9 | Startup and mission integration | Partial: the corrected loaders have real consumers, but a complete startup/mission/game/player/AI/script lifetime is not present. Preserve existing explicit unsupported boundaries until those owners and source implementations exist. |
| 10 | Materials, lighting and fog | Stage 5b implements an unlit live-material subset: independent queue/blend decisions, alpha tests, opacity, KEEP_DIFFUSE, sidedness and refreshed parameters. Level/first-group vertex lighting, emission color, fog, environment mapping, dynamic driver state and legacy raw-world bridges remain separate. |
| 11 | Portal and occluder visibility | Open: retained portal/occluder geometry must feed actual camera/sector traversal and draw selection. Loading the records alone does not implement `ProcessPortals`, `CullThruPortal` or `ProcessOccluders`. |
| 12 | LOD and mipmaps | Stage 5c connects live owned geometry selection, source fade and current-camera redraw/resize refresh, with explicit cull/re-entry and retained buffer-layout proof. Texture-only LOD switching, Request/cache availability, mipmap construction and sampler LOD/bias remain separate source branches; raw static-world presentation still uses its legacy path. |
| 13 | Camera matrices | Partial: existing `frame_camera` models source camera matrices, active-camera ownership and setters; host drawing still uses its presentation camera. Stage 5d corrects the existing metadata view's lost roll/vertical-forward orientation. Full source-camera presentation must consume the existing scene's active `CameraKey`, viewport/aspect and main/near/backdrop matrices in the actual draw phases, with the required coordinate conversion. |
| 14 | Animation lifecycle | Stage 2 corrects the animation catalog. Production startup invocation of `InitAnimIndex`/`DoneAnimIndex` still needs genuine filename-storage/index-allocation owners. Existing bounded controllers already own eight animation slots and evaluator resources. Complete channel evaluation and multi-slot blending, scale/notes, production resource backends and mission playback remain separate. |
| 15 | Render resource lifetime | Stage 5a implemented the reviewed GPU cache/depth scope. Native texture-manager keys, dirty buffer ranges and measured performance gains are not claimed. |
| 16 | Specialized visual passes | Open: billboards, morph rendering, shadows, projectors, particles and mirrors require their own source lifecycle and draw states. Regular mesh depth/cull/blend rules cannot be copied to all of them. |
| 17 | Collision guard order | Open: unsupported world-space geometry is currently rejected before hierarchy rejection. Moving that guard alone would consume constructor/sentinel bounds and introduce false misses; implement the genuine world-space SetSpace/bounds producer first, then preserve source query/effect order. |
| 18 | General LS3DF collision | Open: implement the reference query flags, geometry variants, deformation and color-key rules with actual owned buffers. Existing vehicle/game collision helpers are not the entire LS3DF API. |
| 19 | LS3DF sphere queries | Open: the LS3DF sphere branches need their own traversal, flags and contact outputs. The existing Game dynamic-sphere implementation has a different owner and contract. |
| 20 | Missing-axis quaternion Make | Stage 4a implemented the source cross products and fallback order through actual frame-scale consumers. Existing unsupported numeric domains remain explicit. |
| 21 | Analog input | Open: `ijoy` source axis/dead-zone behavior needs a genuine device/input owner and a connected control consumer. Keyboard values or a synthetic callback alone do not prove analog input support. |
| 22 | Sound states | Stage 6 connects STOP/PLAY/PAUSE/RESUME/NOCAMERA, logical clock and PCM voice behavior. The host clock and decoder timing remain explicit presentation boundaries. |
| 23 | Sound source replacement | Stage 6 releases the old source/voice before opening a different source, retains exact-name fast-path behavior and separately handles shared case-insensitive source caching. |
| 24 | Sound transforms/listener | Stage 6 uses inherited world scale and the actually drawn camera's listener position/sector, including guarded derived range updates. A full original camera controller is workstream 13. |
| 25 | Sound environments and effects | Partial: Stage 6 fixes cone normalization/derived ranges. Area/sector fades, reverb, Doppler, master volume, priority and original seek-block alignment remain separate. |
| 26 | IShow/video | Open: the source player/stream lifecycle requires an actual decoder, clock, surface and ownership backend. A timer-only logo player or placeholder surface is not an implementation of those dependencies. |
| 27 | Archive WAV fallback | Stage 7b implements inner raw/nonzero-LZ dispatch, header/partial-output prefixes, limits and actual stream consumers. Existing PCM16 predictor scope and unsafe/truncated native domains are unchanged. |
| 28 | Original source images | Stage 7a implements the cited BMP/TGA branches and their real texture-factory metadata. BI_BITFIELDS/RLE/alternate palettes and other excluded branches were not inferred. |
| 29 | Write-mode rw_data | Stage 7c implements shared open/write/seek/close behavior and an explicit bounded output owner. It does not turn Installation into a writer or add game-save serialization/atomic durability. |
| 30 | Native byte filenames | Open: archive lookup uses raw byte strings, `strupr`, checksum/suffix matching and `strcmp`; loose/write IO uses `CreateFileA`. `Entry.native_name` preserves bytes, but the CRT case mapping and OS ANSI/OEM path environment are not established by the available source bodies. Removing ASCII guards or guessing CP1250 uppercase would not reproduce this contract. |
| 31 | Data language | Stage 7d connects version selection, raw text, glyphs and selected fonts. The mission-state `LanguageSelect(-1)` branch, credits, profile UI and native mutable text lifetimes remain separate. |

## Dependency order for later stages

1. Preserve the reviewed dependency chain Stage 5a -> 5b -> 5c -> 8a, with
   Stage 5d independently based on 5b. The integration branch combines these
   with the independent loader, image, audio, math, language and IO stages.
   Keep the combined ordinary-material unsupported mask
   `0x80000 | 0x01000000 | 0x04000000`: KEEP_PIXELS and the supported alpha/add
   flags must not reappear as unsupported effects when combining branches.
2. Connect the existing source-image/BPP and effective-alpha results to
   live-model material admission. Extend ownership separately for UV-only LOD
   and unsupported dynamic visual scalars. Stage 7a's existing texture-factory
   BPP/effective-alpha producer must not be implemented a second time.
3. Connect the existing canonical active-camera owner to presentation and source
   visibility traversal. Then
   validate main, near and backdrop passes, portal transitions and occlusion
   with the same retained scene that supplies collision and audio sectors.
4. Add lighting/lightmaps/fog and specialized visual passes separately. Each
   pass must prove its own cull/depth/blend changes, lifetime and failure path.
5. Extend collision after the correct world-space bounds and deformation
   producers exist. Exercise rejected bounds, supported hits and unavailable
   geometry separately; a false miss must not hide a missing implementation.
6. Complete animation, mission/script and game ownership in connected stages.
   Retain explicit errors where the pinned reference itself has only a stub or
   where a required external/device backend is not yet supplied.
7. Add analog input, video and the remaining sound effects with actual backends
   and visible/audible consumers. Source-only unit fixtures are useful but must
   not be presented as interactive original-game validation.

The reference is the specification for this task; it is not a blanket reason
to change unrelated game behavior or infer missing functions. Each next PR
should name its source branch, supported input domain, actual consumer,
failure/cleanup behavior and a useful acceptance test before expanding scope.

Some remaining work has a concrete reference or backend limit. For example,
[`Main.cpp:192-208`](https://github.com/Marvisak/mafia-re/blob/29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c/game/Main.cpp#L192-L208)
marks `SceneCallback` and `GameLoopSingle` as stubs, and
[`g_loadsave.cpp`](https://github.com/Marvisak/mafia-re/blob/29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c/game/ui/g_loadsave.cpp)
still has stub profile/save/menu operations. Their missing behavior cannot be
recovered from those stubs alone. The source
[`IShow.cpp`](https://github.com/Marvisak/mafia-re/blob/29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c/ls3df/IGraph/IShow.cpp)
calls Bink and DirectShow/DirectSound providers; the
[`ijoy.cpp`](https://github.com/Marvisak/mafia-re/blob/29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c/ijoy/ijoy.cpp)
path uses an actual DirectInput device. Those boundaries need an implemented
provider in the target runtime. The lighting, visibility and collision stages
also have substantial source code available; their remaining status reflects
unimplemented connected owners/passes, not a claim that the reference is empty.

For lightmaps, the constructor in
[`I3d_lit_object.cpp:40-50`](https://github.com/Marvisak/mafia-re/blob/29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c/ls3df/I3D/visuals/I3d_lit_object.cpp#L40-L50)
creates six unconstructed lighting levels. Its draw branch supplies a cyan
texture factor; texture alpha, additive and color-key state still have their
own source decisions. The detailed saved-lighting `Load` implementation is
separate from the 4DS geometry record. The inspected implemented callers do not
establish the missing scene-payload association, so implementing the constructor
branch cannot be presented as loading or rendering saved lightmaps.

For filenames,
[`rw_data.cpp:251-322`](https://github.com/Marvisak/mafia-re/blob/29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c/rw_data/rw_data.cpp#L251-L322)
establishes the native archive comparison, while its loose/open-write paths
call `CreateFileA`. The repository's CRT annotations name case-mapping and
codepage functions but do not provide the runtime initialization or mapping
tables needed to choose a non-ASCII host interpretation. This is an explicit
environment dependency, not permission to silently reinterpret native bytes.

For cameras, `frame_camera.rs` already owns the source main/near/backdrop
projection and view matrices. The host presentation callers do not consume an
active `CameraKey`. The reference
[`I3d_camera.cpp:279-303`](https://github.com/Marvisak/mafia-re/blob/29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c/ls3df/I3D/frames/I3d_camera.cpp#L279-L303)
uses the complete world basis; Stage 5d applies that orientation principle to
the existing host metadata view. The pinned source does not establish the
current `intro_pos` metadata-to-active-camera association. Near/backdrop support
also needs real draw partitioning and source depth/pass order, not only matrix
accessors.

## Merge review repair — 2026-10-26

The host write adapter now rejects regular files with multiple hard links
before destructive effects and rechecks the owned file before releasing it on
close. Three public-owner synthetic regressions failed on the original Stage
7c head and pass after the repair. They cover existing output/temporary aliases,
a truncating alias of a live direct stream, and a new temporary alias before
buffered close. Unsupported non-Unix hosts fail closed at construction.

## Review findings already corrected

- A parser-only TGA change did not reach SceneBuilder; Stage 1 adds the real
  decode/dimension/texture path and a consumer regression.
- A discarded patch stream could retain its owned resource; Stage 2 adds the
  correct cleanup path and missing/unrelated/consumed stream cases.
- Updating an older GPU texture generation could evict the cache entry for a
  newer pristine generation; Stage 5a checks the actual GPU allocation identity
  before eviction and exercises repeated old-generation updates.
- Audio derived range fields were updated too early, and cone normalization
  treated small/zero vectors differently from the source; Stage 6 preserves
  constructor fields, guard order and source fallback components.
- A fixed Latin font-family allowlist would reject valid selected resources;
  Stage 7d now reads the validated data suffix and verifies a second family's
  actual glyph widths without substituting the default.
- Stage 5b documentation now distinguishes per-material unlit presentation
  from the source's level/first-group vertex-lighting contract.
- Stage 5c review distinguishes source horizontal FOV from the host renderer's
  vertical FOV, checks the real viewport aspect, and reflects the bootstrap
  hierarchy bbox with the required Z min/max swap. The separate R1 fix validates
  the retained buffer's active owner, descriptor and actual stride before
  selected-LOD bytes are decoded. FVF alone did not prove the stride; regressions
  reject a fresh stride64 buffer but accept a requested64 allocation when source
  reuse actually retained stride32. Source reuse behavior remains intact.
- The source camera view uses all three basis axes. The host metadata view
  retained only forward and forced world-up, losing authored roll and rejecting
  valid vertical-forward orientations. Stage 5d preserves the quaternion's up
  axis through the same coordinate reflection as forward and tests the actual
  metadata-loading consumer.
- Stage 7c's second review found that a buffered physical `.tmp` handle did not
  reserve its logical output. The host adapter now serializes open admission
  and reserves both identities, including failed-open prefixes; real-file and
  two-thread regressions cover direct/buffered/case-alias conflicts. A follow-up
  review also made worker panics fail the concurrency test instead of being
  silently filtered out.
- Stage 8a review required a genuine LMAP class witness and actual view-aware
  and static consumers. It moved sixty derived LOD slots into optional boxed
  storage, keeping that memory and destructor loop out of Plain objects, and
  checks the retained Lit stride, FVF and format before decoding vertex bytes.
- Combined review reproduced a shared-cache bypass: an ordinary material's
  diffuse fallback was cached as though it were a decoded source image, so a
  later Lit draw could skip its required image-decoding error. Only successful
  source decodes now enter the cache. BMP and TGA consumer regressions also
  prove that two ordinary materials retain their different fallback colors.
  This correction is included in Stage 8a itself as well as the integration.

## Historical draft: combined integration

The `fix/staged-mafia-re-integration` review branch combines the actual
published implementation heads without merging anything into `rust-engine`.
It preserves their Git parent relationships and adds explicit integration
commits for shared source interfaces, fixtures and the cross-stage regression.
Its two independent reviews cover these integration changes; each underlying
implementation also has its own sequential review record.

The combined branch is an alternative merge vehicle for the same staged
changes, not an additional independent feature to apply a second time. When
merging stages individually, preserve the dependency chain described above
and carry the integration adjustments as well:

- Retain both original-data chunk and language modules in `lib.rs`.
- Keep `uv_only: None` on the newer material/LOD/Lit fixtures and preserve
  the UV-only admission check when Plain and Lit share the intake path.
- Keep both the UV-only failure test and the ordinary open-ended LOD test.
- Preserve the combined ordinary-material mask and KEEP_PIXELS consumer test.
- Keep actual BMP/TGA decode, dimension/quota checks and the Lit no-fallback
  rule, including successful-decode-only shared caching.
- Retain authored camera up, current viewport-derived LOD FOV and the actual
  presentation camera's audio-listener sector together.

The final combined production revision is
`ceaa4c5367674294d5d5d2df8f29dcdc60ac8564`; the following commit adds this
register only. All thirteen published final implementation heads are ancestors
of that production revision, including the amended Stage 8a head
`effcfeda490bd67b40cf0cdb6733bcff2bbdb7d6`.

The isolated `target-integration` verification on that combined revision is:

| Gate | Actual result |
| --- | --- |
| Required `scripts/test.sh` | Formatting and all-target check pass; overall failure on only the two baseline x87 tests listed below (engine: 1,100 passed, 2 failed, 3 opt-in tests ignored). |
| Separate workspace run with exactly the two baseline names skipped | 1,305 passed: assets 148, installation 8, engine 1,100, main binary 34, program integration 9, renderer 6. Seven GPU tests remain opt-in in this run. Doc tests pass. |
| Workspace/all-target Clippy with `-D warnings` | Pass. |
| Renderer with `--include-ignored` | 9/9 pass, including 3 opt-in GPU regressions. |
| Engine library with `--ignored` | 3/3 GPU consumer tests pass: actual material parameters, camera-selected LOD/fade and owned Lit cyan/raw alpha. |
| Main binary with `--ignored` | 1/1 GPU test passes: actual refresh preserves world draws through LOD count changes, cull and re-entry. |
| Host Python binary-boundary tests | 9/9 pass. |
| Progress tests and generated check | 47/47 and `scripts/progress.py --check` pass; no new progress credits. |

The seven opt-in GPU tests all run on Mesa llvmpipe Vulkan. They verify actual
upload/draw/readback behavior in the available host backend, not native-game
image equivalence, hardware GPU performance or interactive mission completion.
The BMP/TGA fallback-cache regression was first observed failing before the
cache correction and then passed after it; it is also included in the combined
main-binary suite above.

## Historical draft: validation and publication policy

Every implementation stage runs the required `scripts/test.sh`. Formatting,
workspace/all-target compilation, focused regressions, all-target Clippy,
progress tests and the generated progress check have been exercised. GPU stages
also run their opt-in tests against the available Mesa llvmpipe Vulkan device.
These are synthetic host/source-contract checks, not original-game captures.

The following two unchanged tests fail on the pristine starting revision as
well as the implementation branches, including host execution. Their high-FIP
precondition is not satisfied in the available environment:

```text
native_transform::vector_x87::tests::preserves_full_fxsave64_state_with_pending_exception_and_live_sse
native_transform::vector_x87::tests::preserves_long_mode_x87_pointers_across_actual_helpers
```

The full gate is therefore recorded as failing, not relabeled as a pass. A
separate workspace run excludes exactly those two names to check the remaining
tests; the exclusion is a command-line verification step, not a source change.
The synthetic Python binary-analysis tests require host process metadata that
the restricted executor cannot read. All nine pass with approved host visibility.
The 47 progress tests and generated progress check pass without new credits.

Each checkout uses its own Cargo target directory. Sharing a target directory
between independent clones was observed to reuse stale cross-checkout artifacts;
final evidence is taken from the isolated runs.

Publication uses authenticated GitHub Git Database operations because ordinary
git push has no configured credentials in this environment. Every uploaded blob
and complete tree is checked against the reviewed local Git object. Incremental
commit messages and trees are preserved; GitHub-created commit IDs differ from
their local counterparts. No force push or remote merge is performed.

The linked drafts remain reviewable despite the baseline environment failures.
An isolated local integration branch is used to combine the actual published
heads and verify their interaction; it is not a merge into `rust-engine`.
