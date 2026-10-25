# Static-model cache Source-v4 — 2026-10-25

Code `a171632` is accepted after two sequential independent source reviews.

The CPU cache now uses genuine destination models, shared static Mesh/Dummy
resources, directory lookup, cache misses/hits, item-vector growth and explicit
ownership teardown. The actual-data Rust constructor-Scene probe completes43
objects and seven cache entries, then reaches field40a0 at cursor8019 with
missing `base247`. The unloaded original world scene contains that frame among
1415 frames, five sectors and14 portals. Private world-sector/portal candidates
are not part of this accepted implementation.

## Validation and limits

- Source-v4:289 guarded files;1202 Rust tests (119 assets,8 assets integration,
  1036 engine library,26 application,8 engine integration,5 render),9 REA-wrapper
  and47 progress tests; formatting/check/Clippy/application and release builds pass.
- Retained adjacent observations:15 cases/84 snapshots per debug and release,
  12 normal and3 typed prefixes, zero differences. Unsafe ordinal5 is excluded;
  36 retained files are unchanged. These compare directory/CString/array lifecycle,
  not new cache Open, model-entry lifetime or the expanded loader.
- Actual Rust data:43 completed objects, seven cache entries, cursor8019; cache
  heap counts/bytes and six Scene ownership counters return zero after teardown.
  The constructor-only Scene is not the original MissionOpen/world context.
- UI:prior W/A/D/S/Space,three reloads,four cleanups/seven zero CPU counters are
  carried forward only because the final executable is byte-identical. No new UI
  execution. The root-inspected960×640 still shows standalone unlit driving.
- Primary static evidence covers cache flags10/nonNULL thunk, copy traversal,
  naming, array ownership and NULL-animation getter. No new original execution.
  The admitted loader is a single idle static Mesh/Dummy domain with null outer
  callback; general callbacks/animations and changed-resource copy remain open.
- Next:real SceneOpen world producer, live sector/portal ownership and progress
  callback5bcc10 graphics/UI provider. Full MissionOpen, player, scripts, native
  audio and Device14 remain open. No new video, canonical credit or parity claim.

## Previous accepted occluder/light Source-v1 narrative

The following baseline6ef185d current sections are preserved verbatim. Their
code41365c0,1186-test and retained-geometry validation is historical.

Historical source excerpts retain their original Markdown link spelling below.
For current navigation use [coverage](../../COVERAGE.md), [progress](../PROGRESS.md)
and [goals](../GOALS.md).

### README source excerpt

```markdown
The original-data CPU loader now owns occluder geometry, scene membership and
borrowed light registrations. It completes seven records: primary sector, one
occluder and five lights. The next unloaded Model9 is genuinely constructed before
ModelCacheOpen requests an `.i3d` resource and stops at cursor 2086 (Game `54220a`).

This is Rust execution of actual data. Original-runtime equivalence for the
expanded loader, light/ring/HR behavior and nonidentity updates remains unverified.
The visible application remains standalone driving; the campaign is not playable.

Accepted code: 41365c0; [source-bound validation](COVERAGE.md).

```

### COVERAGE source excerpt

```markdown
## Current bounded milestone

The original-data CPU loader now owns occluder geometry, scene membership and
borrowed light registrations. It completes seven records: primary sector, one
occluder and five lights. The next unloaded Model9 is genuinely constructed before
ModelCacheOpen requests an `.i3d` resource and stops at cursor 2086 (Game `54220a`).

This is Rust execution of actual data. Original-runtime equivalence for the
expanded loader, light/ring/HR behavior and nonidentity updates remains unverified.
The visible application remains standalone driving; the campaign is not playable.

Parsed4DS occluder retention is a CPU adapter; complete native LoadImage,
Duplicate, shadow/occlusion rendering and populated cross-scene resources remain open.
Scene rings borrow frames. Diagnostics and Device14 have no native execution claim.
Supplied-PC enumeration skips type12 under partial `0x800`; all-types `0xffff` bypasses it.

Code `41365c0` has two sequential independent non-author source approvals.
Source-v1 passes 1186 Rust tests, 9 REA-wrapper/47 progress tests and all
format/check/Clippy/build checks. Both profiles match retained 19-case/60-snapshot
Build/NULL observations and four enum callback projections. Allocation statistics
are aggregate counts/bytes/call count; exact event order and full enum API are unverified.
The Rust 2086-prefix cleanup has six zero counters after final owner/storage release.
Standalone input covers three reloads and four cleanups with seven CPU counters zero.

[Detailed source-bound evidence](docs/progress.json) ·
[current scope and preceding startup record](docs/history/2026-10-25-occluder-light-loader.md).

```

### PROGRESS source excerpt

```markdown
## Current milestone — 2026-10-25

The original-data CPU loader now owns occluder geometry, scene membership and
borrowed light registrations. It completes seven records: primary sector, one
occluder and five lights. The next unloaded Model9 is genuinely constructed before
ModelCacheOpen requests an `.i3d` resource and stops at cursor 2086 (Game `54220a`).

This is Rust execution of actual data. Original-runtime equivalence for the
expanded loader, light/ring/HR behavior and nonidentity updates remains unverified.
The visible application remains standalone driving; the campaign is not playable.

Code `41365c0` has two sequential independent non-author source approvals.
Source-v1 passes 1186 Rust tests, 9 REA-wrapper/47 progress tests and all
format/check/Clippy/build checks. Both profiles match retained 19-case/60-snapshot
Build/NULL observations and four enum callback projections. Allocation statistics
are aggregate counts/bytes/call count; exact event order and full enum API are unverified.
The Rust 2086-prefix cleanup has six zero counters after final owner/storage release.
Standalone input covers three reloads and four cleanups with seven CPU counters zero.

Complete model-cache loading, full Mission Open, player/camera scheduling, scripts
and objectives remain open. Native Device14, audio and full renderer parity remain open.

The accepted ledger stays **311 scoped entries / 114 contracts**, strict **0/12**;
all six first-mission gates remain open. No whole-function credit is added.

[Coverage](../COVERAGE.md) · [goals](GOALS.md) · [source-bound evidence](progress.json) ·
[current limits and preceding startup record](history/2026-10-25-occluder-light-loader.md).

```

### GOALS source excerpt

```markdown
## Remaining dependency order

1. Complete the genuine ModelCacheOpen boundary reached after seven scene records
   and the next unloaded Model9. Preserve the actual request/cursor and resource
   ownership; current CPU occluder/light loading does not supply a complete mission.
2. Connect original startup/menu/mission owners through genuine graphics, input,
   audio, profiles and tables. Active mission Close and original device providers
   remain open; current file setup is a separate bounded prefix.
3. Extend camera/sector numeric and resource-filled domains through actual
   callers. Native audio/listener, device discard and complete driver/Scene
   shutdown remain separate from CPU ownership and projected evidence.
4. Reconstruct railway GameInit/GameDone and remaining authored actor classes
   with genuine gameplay resources. Existing constructor/base Init/LoadData/
   destruction does not supply those later phases.
5. Bind original Game/player/camera ownership and update scheduling to the
   standalone Car lifecycle. Extend foreign actors, occupants, severe damage,
   particles and projection pools through their real callers.
6. Complete floating operands, LET/IF, all 62 parameter cases and Compile/Init/Done;
   connect detector execution, replay, objectives and completion. The authored
   script probe remains FINDACTOR/UnsupportedWorldBackend at PC 3 / source line 4.
7. Validate the complete first-mission sequence against original runtime and
   interactive Rust runs, including restart, cleanup and required presentation.
   Keep CPU/native projections and complete application/GPU parity distinct.

```
