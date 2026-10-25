# CPU world-sector loader — 2026-10-25

Code `085f31e` / Source-v4 is accepted after two sequential independent source
reviews. The original-data CPU world producer owns 1415 scene members (1394 plain
meshes, 16 dummies, five sectors), 83 hull vertices / 60 faces, 14 portals and 36 planes.
It supplies the genuine `base247` `OBJ_` resource before scene2. After 43 completed
records, field `0x4090` at packet offset 8003 / size 10 requests `LMAP` and stops at
cursor 8013, Game `0x543a4b`. Full visual replacement/lightmap rendering remains unsupported.

Live sector/portal ownership and spatial queries use the original ring, plane and
numeric contracts. Buffer managers retain mutable-key RB topology without sorting.
Unequal alpha follows source dimensions/pitch and native bottom-up nearest-pixel
selection before brightness; the observed material profile is ordinary `0x1384`,
not managed `0x40` or intrinsic-alpha `0x2` / Snow. CPU resource owners are genuine;
their host storage adapters are separate from original graphics/audio/device providers.

## Validation and limits

- 293 guarded source files; 1222 Rust tests (121 assets, 8 assets integration,
  1054 engine library, 26 application, 8 engine integration, 5 render), 9 REA-wrapper
  and 47 progress tests; format/check/Clippy/application and release builds pass.
- Debug/release actual-data outputs agree. The complete scan tracks 1564 live
  frames and 1431 objects including cache templates, 1401 meshes / 1416 levels,
  4020 face groups / 3972 materials. All tracked keys and owned resource counts
  return to zero; backend resources are zero before extra buffer reclamation.
- Safe offline retained-buffer comparison: 15 trajectories / 92 phases, 1182 state
  rows / 92 return projections / 414 ordered effects, zero differences in both Rust
  profiles. Fifteen original files remain unchanged. Defined returns are 50 AL
  booleans and one full 32-bit base/-1; 41 synthetic void markers are not native EAX.
  No new original execution or world/alpha/spatial native equivalence is claimed.
- Actual frozen Source-v4 standalone W/A/D/S/Space, three reloads and Escape:
  four cleanups / seven zero CPU counters; owned processes exit 0 and are reaped.
  Root-inspected 960 × 640 still shows unlit host-chase driving, not world or mission
  rendering. No new video. Stock-runtime feasibility preflight launched no game.
- Cross-scene lifetime requires explicit detach or owner-first destruction;
  native raw unlink does not repair borrowed Scene pointers. Interrupted host
  MakeReady failures reject unsafe retry while native final-plane NULL normal
  return remains admitted. Earlier poisoned failure prefixes are preserved;
  they are not zero-cleanup claims.
- Genuine SceneOpen flags `0x6` / `0x7` loading-progress graphics/UI callback `0x5bcc10`,
  `LMAP` creation/replacement, full MissionOpen/player/scripts/objectives and
  native audio/Device14/GPU parity remain open. 311 entries / 114 contracts,
  strict 0/12 and all six first-mission gates remain unchanged.

## Previous accepted static-model cache Source-v4 narrative

Verbatim current excerpts at baseline `ec4145` / code `a171632`. Their 1202-test and
adjacent15-case record remains historical. Original Markdown paths are displayed
literally; use current [coverage](../../COVERAGE.md), [progress](../PROGRESS.md)
and [goals](../GOALS.md) for navigation.

### README.md source excerpt

```markdown
The original-data CPU loader now loads, copies and caches static Mesh/Dummy
models with genuine shared resources and cache miss/hit/growth ownership. An
actual-data Rust probe completes 43 records and seven cache entries. Its next
lookup, `base247`, belongs to the original world scene, which has not yet been
loaded by the constructor-only Scene probe.

This is static original-code reconstruction and Rust actual-data testing;
original-runtime cache equivalence is unverified. Genuine SceneOpen with live
sector/portal ownership and loading-progress UI is the next dependency.
The visible application remains standalone driving; the campaign is not playable.

Accepted code: a171632; [source-bound validation](COVERAGE.md).

```

### COVERAGE.md source excerpt

```markdown
## Current bounded milestone

The original-data CPU loader now loads, copies and caches static Mesh/Dummy
models with genuine shared resources and cache miss/hit/growth ownership. An
actual-data Rust probe completes 43 records and seven cache entries. Its next
lookup, `base247`, belongs to the original world scene, which has not yet been
loaded by the constructor-only Scene probe.

This is static original-code reconstruction and Rust actual-data testing;
original-runtime cache equivalence is unverified. Genuine SceneOpen with live
sector/portal ownership and loading-progress UI is the next dependency.
The visible application remains standalone driving; the campaign is not playable.

Code `a171632` has two sequential independent source approvals. Source-v4 passes
1202 Rust tests, 9 REA-wrapper/47 progress tests and format/check/Clippy/build checks.
Both profiles match 15 adjacent retained-observation cases; new cache Open and the
expanded loader have no native comparison. The actual-data probe ends at cursor
8019 with zero owned resources after cleanup. Prior standalone controls, three
reloads and four cleanups are carried forward through the byte-identical final
executable; no new UI run is claimed. [Source-bound details](docs/progress.json).

[Current scope and preceding occluder/light record](docs/history/2026-10-25-model-cache-loader.md).

```

### docs/PROGRESS.md source excerpt

```markdown
## Current milestone — 2026-10-25

The original-data CPU loader now loads, copies and caches static Mesh/Dummy
models with genuine shared resources and cache miss/hit/growth ownership. An
actual-data Rust probe completes 43 records and seven cache entries. Its next
lookup, `base247`, belongs to the original world scene, which has not yet been
loaded by the constructor-only Scene probe.

This is static original-code reconstruction and Rust actual-data testing;
original-runtime cache equivalence is unverified. Genuine SceneOpen with live
sector/portal ownership and loading-progress UI is the next dependency.
The visible application remains standalone driving; the campaign is not playable.

Code `a171632` has two sequential independent source approvals. Source-v4 passes
1202 Rust tests, 9 REA-wrapper/47 progress tests and format/check/Clippy/build checks.
Both profiles match 15 adjacent retained-observation cases; new cache Open and the
expanded loader have no native comparison. The actual-data probe ends at cursor
8019 with zero owned resources after cleanup. Prior standalone controls, three
reloads and four cleanups are carried forward through the byte-identical final
executable; no new UI run is claimed. [Source-bound details](progress.json).

Full MissionOpen, player/camera scheduling, scripts and objectives remain open.
Native Device14, audio and full renderer parity remain open.

The accepted ledger stays **311 scoped entries / 114 contracts**, strict **0/12**;
all six first-mission gates remain open. No whole-function credit is added.

[Coverage](../COVERAGE.md) · [goals](GOALS.md) · [source-bound evidence](progress.json) ·
[current limits and preceding occluder/light record](history/2026-10-25-model-cache-loader.md).

```

### docs/GOALS.md source excerpt

```markdown
## Remaining dependency order

1. Load the original world scene through genuine SceneOpen before applying scene2.
   Reconstruct live sector hulls, planes, portals and progress presentation; the
   constructor-only probe reaches `base247`, which exists in the unloaded world.
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
