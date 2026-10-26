# PR merge validation — 2026-10-26

The sections below preserve the preceding current narrative from baseline
`aa3da350991634f7ab1f5636585e8637f1316b1b`. Fenced excerpts retain the original
bytes, including literal relative links, and their earlier validation scope.

[Current coverage](../../COVERAGE.md) · [current progress](../PROGRESS.md) ·
[current goals](../GOALS.md) · [merge validation](../implementation/mafia-re-remediation.md).

## Current source-bound validation

Accepted code `a4dee6d` passes the required gate and **1310 Rust tests
in both debug and release**, including both unchanged x87 tests without
exclusions; 9 Python boundary and 47 progress tests pass. Seven opt-in GPU
checks pass on Mesa llvmpipe Vulkan. New standalone W/A/D/S/Space, three
reloads and Escape finish with four cleanups and seven zero CPU counters.

The current stage scope, CPU probe and separate Lit test limits are recorded in
the [remediation register](../implementation/mafia-re-remediation.md).

## Archived narrative 1: `README.md`

```markdown
Current support includes the Czech menu, bounded original models and skeletal
animation, and standalone original-data driving with collision, vehicle damage,
WAV mixing and explicit cleanup. Rendering is unlit first LOD with a host chase
camera. Original player/camera scheduling, mission scripts and traffic remain open.

The original-data CPU world loader now owns 1415 scene frames, five sectors
and 14 portals with 36 hull planes. It supplies the real `base247` object before
the ordered scene2 loader, which completes 43 records and then reaches the
required `OBJ_` → `LMAP` visual replacement at cursor 8013 (Game `543a4b`).

Live sector/portal ownership, spatial queries, mutable-key buffer trees and
unequal-size alpha maps now share the genuine CPU resource path. Original
SceneOpen presentation, the loading-progress UI and `LMAP` replacement remain
open. This is Rust actual-data execution; native composed-loader parity and
complete MissionOpen are unverified. The visible application remains standalone
driving; the campaign is not playable.

Accepted code: 085f31e; [source-bound validation](COVERAGE.md).

See [current evidence and limits](COVERAGE.md), [progress](docs/PROGRESS.md),
[remaining goals](docs/GOALS.md) and [dated history](docs/history/README.md).

```

## Archived narrative 2: `COVERAGE.md`

```markdown
## Current bounded milestone

The original-data CPU world loader now owns 1415 scene frames, five sectors
and 14 portals with 36 hull planes. It supplies the real `base247` object before
the ordered scene2 loader, which completes 43 records and then reaches the
required `OBJ_` → `LMAP` visual replacement at cursor 8013 (Game `543a4b`).

Live sector/portal ownership, spatial queries, mutable-key buffer trees and
unequal-size alpha maps now share the genuine CPU resource path. Original
SceneOpen presentation, the loading-progress UI and `LMAP` replacement remain
open. This is Rust actual-data execution; native composed-loader parity and
complete MissionOpen are unverified. The visible application remains standalone
driving; the campaign is not playable.

Code `085f31e` has two sequential independent source approvals. Source-v4 passes
1222 Rust tests, 9 REA-wrapper/47 progress tests and format/check/Clippy/build checks.
Both Rust profiles match 15 retained buffer trajectories/92 phases; this is separate
from the actual-data CPU world probes. Those probes reach `LMAP` and release all
tracked resources. New standalone W/A/D/S/Space, three reloads and four cleanups
have seven zero CPU counters. Neither probe nor capture establishes native
SceneOpen, loading UI, mission execution or original rendering parity.

[Current scope and preceding cache record](docs/history/2026-10-25-world-sector-loader.md).

```

## Archived narrative 3: `docs/PROGRESS.md`

```markdown
## Current milestone — 2026-10-25

The original-data CPU world loader now owns 1415 scene frames, five sectors
and 14 portals with 36 hull planes. It supplies the real `base247` object before
the ordered scene2 loader, which completes 43 records and then reaches the
required `OBJ_` → `LMAP` visual replacement at cursor 8013 (Game `543a4b`).

Live sector/portal ownership, spatial queries, mutable-key buffer trees and
unequal-size alpha maps now share the genuine CPU resource path. Original
SceneOpen presentation, the loading-progress UI and `LMAP` replacement remain
open. This is Rust actual-data execution; native composed-loader parity and
complete MissionOpen are unverified. The visible application remains standalone
driving; the campaign is not playable.

Code `085f31e` has two sequential independent source approvals. Source-v4 passes
1222 Rust tests, 9 REA-wrapper/47 progress tests and format/check/Clippy/build checks.
Both Rust profiles match 15 retained buffer trajectories/92 phases; this is separate
from the actual-data CPU world probes. Those probes reach `LMAP` and release all
tracked resources. New standalone W/A/D/S/Space, three reloads and four cleanups
have seven zero CPU counters. Neither probe nor capture establishes native
SceneOpen, loading UI, mission execution or original rendering parity.

Full MissionOpen, player/camera scheduling, scripts, objectives, native audio and
Device14 remain open.

The accepted ledger stays **311 scoped entries / 114 contracts**, strict **0/12**;
all six first-mission gates remain open. No whole-function credit is added.

[Coverage](../COVERAGE.md) · [goals](GOALS.md) · [source-bound evidence](progress.json) ·
[current limits and preceding cache record](history/2026-10-25-world-sector-loader.md).

```

## Archived narrative 4: `docs/GOALS.md`

```markdown
## Remaining dependency order

1. Reconstruct the genuine `LMAP` visual factory/replacement reached after CPU
   world loading and 43 scene2 records. Complete SceneOpen progress presentation;
   CPU geometry/resources do not supply the original loading UI or MissionOpen.
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

## Archived narrative 5: `README.md`

```markdown
## Actual Rust captures

![Standalone original-data driving after CPU world-loader changes](docs/screenshots/2026-10-25-world-sector-prefix.webp)

Actual new standalone driving/reload capture; host chase camera and unlit rendering.
The CPU world probe and retained-buffer comparisons are validated separately.
This is not mission or original-world rendering.
[Capture evidence](docs/screenshots/README.md).

[![Original taxi during the continuous driving capture](docs/screenshots/2026-10-24-car-driving-verified.webp)](docs/videos/2026-10-24-car-driving.mp4)

[Existing 18-second driving video](docs/videos/2026-10-24-car-driving.mp4)
· original-resource stereo audio · [dated scope and timing limits](docs/screenshots/README.md#2026-10-24--continuous-driving-preview).

```
