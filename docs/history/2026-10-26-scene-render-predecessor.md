# Scene/render documentation predecessor — 2026-10-26

These six complete predecessor files are preserved verbatim from the documentation
baseline before the canonical scene/render milestone. Each ten-backtick fence
contains the original complete file bytes; its original relative links are left
unchanged. Dates, validation and implementation limits inside describe that earlier
revision. Current scope is recorded in [coverage](../../COVERAGE.md) and
[progress](../PROGRESS.md). Earlier history and media remain in the
[history index](README.md).

| Original file | SHA-256 | Bytes |
| --- | --- | ---: |
| `README.md` | `896f169cfa216b825099cd98a07f482a6dbac7d26865fed1002da7c2ce5caf53` | 7740 |
| `COVERAGE.md` | `207c1bbc8abd89f7fb05dd7199c1dad1376b47042fea300c68697eb4372a4fb7` | 3553 |
| `docs/GOALS.md` | `d39c8b25f40ef58e03553c430c98d5ff66eb8baeb13fef14fae719f570406f27` | 3157 |
| `docs/REFERENCE_PROJECTS.md` | `de8663746ddb263af31ee7b2e1f08c84aa78bae471f20a4910cd0b80235e70bd` | 50416 |
| `docs/PROGRESS.md` | `49647781ecd7337d9bcd2e3ef6217d09d237b3918321a0407932277b786eda71` | 106247 |
| `docs/implementation/mafia-re-remediation.md` | `a4a8a0e0a29a266e243e93876ea4c281f7264f2c1bf5f265386d2e208f38877a` | 30435 |

<a id="readme"></a>

## README.md

``````````markdown
# Lost Heaven Reborn

An independent Rust reimplementation of Mafia: The City of Lost Heaven (2002),
using the user's legally obtained original PC assets. The immediate goal is the
playable first mission, from the original menu through completion. **The campaign
is not playable; all six first-mission gates remain open.**

Current source-based stages add original-data loader and image fixes, live
material parameters and camera-selected model LOD, authored view orientation,
audio lifecycle, archive/write IO and data-language selection. Lit objects use
the source's unconstructed cyan branch; saved lightmaps and their scene `LMAP`
payload association remain open. Original lighting, visibility, player/camera
scheduling and authored mission scripts remain incomplete. The separate legacy
raw static-world presentation remains unlit first LOD.

The visible application supports inspection and standalone original-data driving.
No native-function, scoped-port or full-system credit is added by these stages.

[Source scope and merge validation](docs/implementation/mafia-re-remediation.md) ·
[current evidence and limits](COVERAGE.md) · [progress](docs/PROGRESS.md) ·
[remaining goals](docs/GOALS.md) · [dated history](docs/history/README.md).

<details>
<summary>Static research and accepted scoped-port dashboard</summary>

<!-- generated-function-progress:start -->
As of 2026-10-25 · snapshot `RUST-SCOPED-PORTS-20261025-47`.

<p align="center"><img src="docs/progress.svg" alt="Progress dashboard: static retained pseudocode, reviewed scoped Rust entries, and strict system verification" width="100%"></p>

| Original binary | Static retained pseudocode | Denominator basis | Full contracts | Full Rust ports | Differential runtime verified |
| --- | ---: | --- | ---: | ---: | ---: |
| Game.exe | 4784 / 4,914 (97.35%) | Explicit profile union | N/A | N/A | N/A |
| rw_data.dll | 176 / 234 (75.21%) | Explicit profile union | N/A | N/A | N/A |
| LS3DF.dll | 1555 / 1,714 (90.72%) | Explicit profile union | N/A | N/A | N/A |
| Setup.exe (supplied configuration) | 172 / 172 (100.00%) | In-image non-thunk procedures | N/A | N/A | N/A |

Strict full-system gate: **0/12**. Binary-level static counts use the named denominator basis and do not combine into a game-completion percentage. Unknown denominators remain N/A. 1 supplied configuration Setup inventory uses 172 in-image non-thunk procedures. Separately, archive installer bootstrap static inventory: **376 / 376 (100.00%)**; this distribution target is not combined with engine-library counts. The campaign is not playable.

Accepted scoped Rust ports: **311 entries across 114 reviewed contracts**. These remain separate from retained pseudocode and whole-function counts.

Regenerate locally with `python3 scripts/progress.py --write`; verify without writes with `python3 scripts/progress.py --check`. Versioned snapshots are retained in `docs/progress.json`.
Milestone history and scoped implementation evidence are in [docs/PROGRESS.md](docs/PROGRESS.md).
Supplied binary catalog (file identities; independent measures):

| Binary | Platform | Variant | Role | Static retained pseudocode | Scoped Rust entries |
| --- | --- | --- | --- | ---: | ---: |
| DrvMgt.dll | PC | Installed game | Driver-management helper, exact runtime use unresolved | N/A | 0 |
| Game.exe | PC | Installed game | Game and mission executable | 4784 / 4,914 (97.35%) | 149 |
| IJoy.dll | PC | Installed game | DirectInput joystick wrapper | N/A | 0 |
| INet.dll | PC | Installed game | Network helper, exact runtime use unresolved | N/A | 0 |
| LS3DF.dll | PC | Installed game | 3D, animation, audio and rendering engine | 1555 / 1,714 (90.72%) | 145 |
| Setup.exe (supplied configuration) | PC | Installed game | Installed game configuration utility | 172 / 172 (100.00%) | 5 |
| binkw32.dll | PC | Installed game | Bink video codec boundary | N/A | 0 |
| ogg.dll | PC | Installed game | Ogg container library boundary | N/A | 0 |
| rw_data.dll | PC | Installed game | Game archive and data access library | 176 / 234 (75.21%) | 12 |
| vorbis.dll | PC | Installed game | Vorbis codec library boundary | N/A | 0 |
| vorbisfile.dll | PC | Installed game | Vorbis stream and file library boundary | N/A | 0 |
| Setup.exe (archive installer bootstrap) | PC | Distribution archive | Distribution installer bootstrap | 376 / 376 (100.00%) | 0 |
| SLUS_206.71 | PS2 | PS2 disc executable | PS2 game executable with retained linker symbols | N/A | 0 |
| CDVDSTM.IRX | PS2 | PS2 disc IOP module | CD and DVD streaming support module | N/A | 0 |
| IOPRP271.IMG | PS2 | PS2 disc support image | IOP reboot and support image, container contents unmeasured | N/A | 0 |
| LIBSD.IRX | PS2 | PS2 disc IOP module | Sound support module | N/A | 0 |
| MCMAN.IRX | PS2 | PS2 disc IOP module | Memory-card management module | N/A | 0 |
| MCSERV.IRX | PS2 | PS2 disc IOP module | Memory-card service module | N/A | 0 |
| MTAPMAN.IRX | PS2 | PS2 disc IOP module | Multitap support module | N/A | 0 |
| PADMAN.IRX | PS2 | PS2 disc IOP module | Controller support module | N/A | 0 |
| SDRDRV.IRX | PS2 | PS2 disc IOP module | Sound driver module | N/A | 0 |
| SIO2MAN.IRX | PS2 | PS2 disc IOP module | SIO2 support module | N/A | 0 |
| SOUND.IRX | PS2 | PS2 disc IOP module | Game-distributed sound module | N/A | 0 |
| SOUND2.IRX | PS2 | PS2 disc IOP module | Game-distributed sound module | N/A | 0 |
| SOUND_M.IRX | PS2 | PS2 disc IOP module | Game-distributed sound module | N/A | 0 |

N/A means no measured static procedure ratio. PS2 linker symbol research supplies naming evidence, not static procedure coverage. Scoped entries are absolute reviewed counts joined by binary SHA-256; zero is not 0% coverage. Whole-function and differential runtime coverage remain unmeasured (N/A).
<!-- generated-function-progress:end -->

</details>

## Build and run

Rust 1.99.1 is pinned. Linux uses wgpu/Vulkan and ALSA development headers.
Dependencies are locked; original executables and REA are development inputs,
not runtime dependencies. Native macOS/Metal remains untested; the Linux Apple
cross-check is blocked by missing CoreAudio SDK headers.

```sh
./scripts/setup.sh
./scripts/test.sh
./scripts/run.sh --root /path/to/Mafia
./scripts/run.sh --root /path/to/Mafia --model models/taxi00.4ds --car-driving
./scripts/run.sh --root /path/to/Mafia --model models/TommyTAXIDRIVER.4ds --animation anims/breath01a.5ds --skeletal-animation
```

Driving: W/S accelerate/brake, A/D steer, Space handbrake, R reload.
Inspection: arrows and W/S move the camera. Menu New Game opens static inspection;
it does not start the authored mission. `--mission mise01` is also inspection.
Add `--headless --capture /tmp/scene.png` for a still or `--frames N` for a bounded
window run. [More commands and historical validation](docs/history/2026-10-25-readme-narrative.md).

## Actual Rust captures

![Standalone original-data driving after the staged merge](docs/screenshots/2026-10-26-pr-merge-validation.webp)

[New capture and interactive scope](docs/screenshots/README.md) ·
[existing 18-second driving video](docs/videos/2026-10-24-car-driving.mp4).
The dated video retains its original scope; this merge adds no video.

## Research and assets

REA/Ghidra and actual PS2 symbols guide reconstruction; scoped contracts,
static pseudocode and complete-system validation are separate measures.
Original data, executables and private research stay outside Git; public tests
use synthetic fixtures. [Reference audit](docs/REFERENCE_PROJECTS.md) ·
[optional enhanced assets](docs/ASSET_ENHANCEMENT.md) · [third-party notices](THIRD_PARTY_NOTICES.md).

Code is MIT licensed. Game assets and trademarks belong to their owners.
``````````

<a id="coverage"></a>

## COVERAGE.md

``````````markdown
# Reconstruction coverage

**311 accepted scoped entries / 114 contracts; strict full-system gate 0/12.**
Whole-function coverage remains N/A. These measures do not form an overall
completion percentage. The campaign and first mission remain unplayable.

## Current bounded milestone

The staged source-based merge connects original-data loaders and image intake,
live material parameters and camera-selected model LOD, authored view
orientation, audio lifetime, archive/write IO and language selection. Lit objects
use the unconstructed cyan source branch. Saved lightmaps, scene `LMAP` payload
association and original lighting/visibility remain open.

Accepted code `a4dee6d` passes the required gate and **1310 Rust tests
in both debug and release**, including both unchanged x87 tests without
exclusions; 9 Python boundary and 47 progress tests pass. Seven opt-in GPU
checks pass on Mesa llvmpipe Vulkan. New standalone W/A/D/S/Space, three
reloads and Escape finish with four cleanups and seven zero CPU counters.

The visible application remains inspection and standalone driving. Source tests,
GPU checks and actual UI input have separate scopes; they do not establish
original-game image/audio parity, complete SceneOpen/MissionOpen or a playable
mission. [Stage scope and validation](docs/implementation/mafia-re-remediation.md) ·
[preceding current narrative](docs/history/2026-10-26-pr-merge-validation.md).

## Strict system registry

Each row needs original evidence, implementation and relevant runtime validation.
Partial support below does not pass a system gate.

| System | Current bounded support | Remaining gate | Verified |
| --- | --- | --- | --- |
| Assets and world streaming | DTA/model/scene intake; nearby world geometry | Full Game/world streaming and mission ownership | No |
| Rendering and materials | Software Vulkan, live unlit materials/model LOD; unconstructed Lit | Original lighting/material/GPU behavior; Metal | No |
| Animation | Owned hierarchy, one sequential controller slot, CPU skinning | Blending, scale/notes and gameplay selection | No |
| Collision and physics | Original-road and bounded dynamic contacts | General actor/world response breadth | No |
| Vehicles | Standalone lifecycle, driving, damage and cleanup | Player/mission ownership, occupants, severe-damage paths | No |
| Characters and combat | Base/source-only and bounded railway lifecycle; ordered loader | Authored derived actors and combat | No |
| Traffic and pedestrians | Resource presence only | Spawn, routing and simulation | No |
| AI, navigation and police | Selected manager initialization/reset | Resource-backed AI, navigation and pursuit | No |
| Scripts and cutscenes | Parsing, bounded scheduler/lookup/replay dependencies | Compiler, original scheduling, objectives and completion | No |
| Camera and input | Host driving controls/chase view over recovered setters | Original player/camera scheduling | No |
| Audio, UI and localization | Selected data-language menu, PCM16 WAV mixing/capture | Codecs, HUD, profile/settings and hardware verification | No |
| Save and load | No persistence implementation | Schema, save/load and recovery | No |

Actual UI evidence is separate from native CPU projections. Host CW/MXCSR,
original rendering/audio parity and native macOS/Metal are unclaimed.
[Current screenshot](docs/screenshots/2026-10-26-pr-merge-validation.webp) ·
[full dated validation history](docs/history/2026-10-25-coverage-narrative.md) ·
[progress](docs/PROGRESS.md) · [acceptance goals](docs/GOALS.md).
``````````

<a id="goals"></a>

## docs/GOALS.md

``````````markdown
# Reconstruction goals

Build a standalone Rust engine that runs the original PC campaign from the user's
own assets. The immediate goal is the playable first mission from the original
menu through completion. Static inspection and standalone driving do not satisfy it.

## First-mission acceptance

All six gates remain open. [Coverage](../COVERAGE.md) records the separate strict
12-system registry; accepted scoped work remains 311 entries / 114 contracts.

| Gate | Required evidence |
| --- | --- |
| Original startup | Menu starts MISE01 with original actors, resources, scripts and update order. |
| Playable control | Original player/taxi movement, camera, collision and damage respond correctly. |
| Mission progression | Original objectives, pursuit, events and replay transitions reach completion. |
| Presentation needed for play | Models, animation, materials, text/HUD and required audio support play. |
| Repeatability | Restart/repeated runs match representative original-runtime and interactive sequences. |
| Standalone execution | User assets suffice; original executables and REA remain development-only inputs. |

Linux software Vulkan is exercised. Native macOS/Metal remains untested; the Linux
Apple cross-check is blocked by missing CoreAudio SDK headers.

## Remaining dependency order

1. Connect saved-lightmap loading to the actual scene `LMAP` payload and visual
   replacement. Unconstructed Lit drawing does not supply saved lighting.
   Complete SceneOpen progress presentation and genuine MissionOpen owners.
2. Connect original startup/menu, player/camera, graphics, input, audio,
   profiles and tables. Extend source lighting, fog, specialized visual passes
   and portal/occluder visibility through the retained scene.
3. Complete authored actor, animation, collision and vehicle lifetimes through
   real mission owners, including remaining GameInit/GameDone dependencies.
4. Complete script expressions, parameter/compiler/lifecycle cases, scheduler,
   world effects, replay, objectives and mission completion. The authored MISE01
   probe remains FINDACTOR/UnsupportedWorldBackend at PC 3 / source line 4.
5. Validate the complete first mission against original-runtime and interactive
   Rust sequences, including restart, cleanup and required presentation. Keep
   CPU/source projections, GPU checks and complete application parity distinct.

## Reconstruction rules

Use REA/Ghidra contracts keyed by binary hash and canonical entry. Actual PS2
symbols establish names; PC layout, ABI and behavior need separate evidence.
Implement dependencies through real owners, compare admitted domains, and keep
unsupported paths explicit. Two sequential independent reviews precede acceptance.
Original models/missions are general regression inputs, not implementation special
cases. Static pseudocode, scoped ports and complete systems remain separate counts.

[Current progress](PROGRESS.md) · [technical reference audit](REFERENCE_PROJECTS.md) ·
[previous detailed goals](history/2026-10-25-goals.md).
Optional [enhanced assets](ASSET_ENHANCEMENT.md) follow original-data compatibility
and the playable-mission goal.
``````````

<a id="reference-projects"></a>

## docs/REFERENCE_PROJECTS.md

``````````markdown
# Reference-project audit

## Current CPU world-sector scope — 2026-10-25

Code `085f31e` supplies 1415 genuine CPU world frames with live sectors/portals,
spatial queries, mutable-key buffer trees and unequal alpha processing. The real
`base247` now reaches required `OBJ_` → `LMAP` replacement after 43 scene2 records.
Original SceneOpen loading UI, native composed-loader parity and MissionOpen
remain open. [Scope and preceding record](history/2026-10-25-world-sector-loader.md).
Observed mafia-re `1ee42a56` and behavior pin `86a7b15c` remain unchanged.

## Current static-model cache scope — 2026-10-25

Code `a171632` reconstructs bounded cache miss/hit/copy/growth and advances the
actual-data constructor-Scene probe to43 records/seven entries and missing
`base247`. Original world SceneOpen and loading UI remain dependencies.
New cache/native loader parity is unverified; adjacent retained comparisons keep
their narrower scope. [Evidence and preceding record](history/2026-10-25-model-cache-loader.md).
Observed mafia-re `1ee42a56` and behavior pin `86a7b15c` remain unchanged.

## Current occluder/light scope — 2026-10-25

Code `41365c0` advances the actual-data Rust scene loader through seven records
and an unloaded Model9 to ModelCacheOpen. Current comparisons reuse retained
original observations; ring/HR/light/nonidentity and the new 2086 native prefix
remain unverified. [Scope, proof partitions and preceding startup record](history/2026-10-25-occluder-light-loader.md).
Observed mafia-re `1ee42a56` and behavior pin `86a7b15c` remain unchanged.

## Fresh mafia-re inspection — 2026-10-25

Observed [`1ee42a56`](https://github.com/Marvisak/mafia-re/tree/1ee42a56214afeb54c536b85a4b16b5c4f4d86e3), nine commits/81 files beyond `109954d6`.
All 27 new unique annotated addresses have original-PC primary receipts and
bounded Rust callers/owned-state implementations; this is not whole-function parity.
The behavior pin remains `86a7b15c`; supplied-PC CheckCD returns true and the
upstream drive/retry tail is unreachable. See the [17-family frontier and evidence
limits](history/2026-10-25-original-startup-mission-audit.md). Older audits follow unchanged.

## Previous source audit — 2026-10-25

Rechecked all three requested projects against Rust baseline `261ff0d`.
Repository metadata and branch heads were freshly fetched; inspected source and
per-file hashes are retained privately. This was source inspection only: no
reference build, comparison regeneration, REA analysis or original execution.
The Rust ledger remains **311 scoped entries / 114 contracts**, strict **0/12**.
The mafia-re behavior reference stays pinned to `86a7b15c`; observing new source
neither advances that pin nor grants implementation credit.

| Project | Freshly observed revision | Useful contribution and boundary |
| --- | --- | --- |
| [mafia-re](https://github.com/Marvisak/mafia-re/tree/109954d6aaf7dd36f2d9dacb336797c0d062a925) | `109954d6`, master; 17 commits beyond `9b793967` | New `main` body maps menu, intro and MISE01 callers. Mission loading, initialization and the game loop still have explicit stubs. |
| [MafiaUnity](https://github.com/MafiaHub/MafiaUnity/tree/0d7a12f03ce4385173377c3e9f457dcc2e891f24) | `0d7a12f0`, develop; unchanged, archived | Scene resource composition, name references and a separate patch pass. Unity lifetime and prototype actors do not supply native Scene ownership or mission execution. |
| [OpenMF-Archived](https://github.com/MafiaHub/OpenMF-Archived/tree/7167e74d65eeced643aa51fb56bb62b8556d8389) | `7167e74d`, master; archived | Inspectable scene/name/KLZ joining and format utilities. Its abandoned renderer/physics/player prototypes do not replace the existing Rust implementation. |

### New mafia-re findings

The 17-commit delta changes 48 files. The most useful addition is
[`main`'s NEW_GAME path](https://github.com/Marvisak/mafia-re/blob/109954d6aaf7dd36f2d9dacb336797c0d062a925/game/Main.cpp#L150-L179):
close the old mission, open intro, load its collision, initialize and run it,
close it, then open MISE01, load collision, reset new-game state, initialize,
run and close. This is a source map for subsequent Game.exe research.
[`GameLoopSingle`](https://github.com/Marvisak/mafia-re/blob/109954d6aaf7dd36f2d9dacb336797c0d062a925/game/Main.cpp#L38-L44),
[`InitSystem`](https://github.com/Marvisak/mafia-re/blob/109954d6aaf7dd36f2d9dacb336797c0d062a925/game/Main.cpp#L350-L365)
and [Mission Open/Close](https://github.com/Marvisak/mafia-re/blob/109954d6aaf7dd36f2d9dacb336797c0d062a925/game/Mission.cpp#L1-L21)
are stubs; Open returns an error and InitSystem returns false. The committed
Game card reports nine matched bodies, not a working campaign. Scene, IGraph
and ITexture bodies are unchanged from the previous observed revision; the
LS3DF delta mainly adjusts compiler-expression matching.

The [Scene constructor](https://github.com/Marvisak/mafia-re/blob/109954d6aaf7dd36f2d9dacb336797c0d062a925/ls3df/I3D/I3d_scene.cpp#L88-L180)
requests `CreateITexture("Snow.tga", NULL, 2, ...)`. This is the immediate,
image-alpha path; managed creation uses `0x40`. The
[filename factory](https://github.com/Marvisak/mafia-re/blob/109954d6aaf7dd36f2d9dacb336797c0d062a925/ls3df/IGraph/IGraph.cpp#L570-L622)
leads to cache matching, retained references, map-directory lookup, Create and
failure Release. Its upstream address is LS3DF VA `0x1006c2c0`; the reached
[Create path](https://github.com/Marvisak/mafia-re/blob/109954d6aaf7dd36f2d9dacb336797c0d062a925/ls3df/IGraph/ITexture.cpp#L223-L310)
is VA `0x10075f50`. These addresses and flag labels remain source leads until
independently checked against original PC instructions and naming evidence.
Our [material adapter](../crates/lhr-engine/src/loaded_visual_resources.rs)
explicitly rejects immediate bit `2` because source-BPP/alpha behavior remains
unresolved. Existing managed-record creation does not close that dependency.

### MafiaUnity and OpenMF findings

MafiaUnity's [MissionManager](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Core/Managers/MissionManager.cs#L33-L88)
loads scene.4ds, cache, scene2 and KLZ; its
[SceneGenerator](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Internal/Generators/SceneGenerator.cs#L42-L200)
registers names before patching references and selectively applying local
transforms. That suggests duplicate-name, external-reference and patch-order
regression inputs. The names are not an original actor registry. Its
[actor dispatch](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Game/ObjectDefinition.cs#L107-L150)
covers Physical, Character and Door; Player, Car and original mission-script
execution are absent from that dispatch. C# mod `ScriptMain` is a separate
mechanism. Unload delegates to Unity object/resource lifetime; there is no
native shared Snow or thirteen-effect Scene Close implementation here.

OpenMF's [mission assembly](https://github.com/MafiaHub/OpenMF-Archived/blob/7167e74d65eeced643aa51fb56bb62b8556d8389/components/mission/mission_impl.cpp#L18-L122)
and [collision-name lookup](https://github.com/MafiaHub/OpenMF-Archived/blob/7167e74d65eeced643aa51fb56bb62b8556d8389/components/mission/mission_impl.cpp#L193-L287)
are useful cross-checks for data relationships. Its
[special-record merge](https://github.com/MafiaHub/OpenMF-Archived/blob/7167e74d65eeced643aa51fb56bb62b8556d8389/components/scene2_bin/parser_scene2bin.cpp#L77-L103)
requires an already-seen base object and drops unmatched specials.
[Character support is TODO and player start creates a generic visual](https://github.com/MafiaHub/OpenMF-Archived/blob/7167e74d65eeced643aa51fb56bb62b8556d8389/components/mission/mission_impl.cpp#L310-L390).
[KLZ conversion](https://github.com/MafiaHub/OpenMF-Archived/blob/7167e74d65eeced643aa51fb56bb62b8556d8389/components/klz/bullet_klz.cpp#L71-L189)
assumes grouped first-link faces and hardcodes cylinder height; the
[renderer](https://github.com/MafiaHub/OpenMF-Archived/blob/7167e74d65eeced643aa51fb56bb62b8556d8389/components/4ds/osg_4ds.cpp#L227-L248)
reduces skin/morph forms to standard geometry. No connected original mission VM
was found in the inspected engine loop. Keep our native-backed collision,
animation and archive behavior; use these projects to challenge missing data
relationships rather than adopt their prototype mechanics.

The next bounded implementation and its acceptance criteria are listed under
[Next general-engine work](#next-general-engine-work).

## Historical source audit — 2026-10-22/23

The following dated observations retain their original implementation limits;
they do not describe the current standalone driving milestone.

Historical source audit refreshed on 2026-10-22 against Rust engine commit [`5903942`](https://github.com/dnesdan/lost-heaven-reborn/commit/5903942266c212c6cd5e88b75df688973e6b447b), extending the earlier audit at `a739570`. Four independent research assignments inspected mafia-re and MafiaUnity formats, runtime and rendering. That historical inspection retained the same mafia-re revision. A source-only refresh on 2026-10-23 observes upstream `0c44be32dd152c9722bbd8c2c8e18f112a279c04`, 19 commits beyond the behavior pin. It adds instruction matching, a Game.exe bootstrap and additional version targets; no upstream build, regenerated comparison or original execution was performed for this refresh. No reference application was built or run, and this refresh performed no new REA analysis or original-runtime comparison. The audit baseline was 110 scoped entries across 41 contracts; the audit itself adds no implementation or gameplay credit. The later bounded frame/model lifecycle snapshot retained 117 scoped entries across 43 contracts. The subsequent resource-primitives snapshot retained 126 scoped entries across 44 contracts. The subsequent mesh-object snapshot retained 129 scoped entries across 45 contracts. The buffer-manager snapshot retained 147 scoped entries across 46 contracts. The subsequent standalone bridge retained 147 scoped entries across 46 contracts with zero new canonical entries. The per-object plain LOD snapshot retained 151 scoped entries across 47 contracts. The nonmanaged texture/string snapshot retained 159 scoped entries across 49 contracts. The managed-removal snapshot retained 160 scoped entries across 50 contracts; managed-record creation retained 161 entries across 51 contracts. The Request-producer snapshot retained 162 entries across 52 contracts. The standalone-unload snapshot retained 163 entries across 53 contracts. The prior release snapshot retained 164 entries across 54 contracts. The Restore snapshot retained 165 scoped entries / 55 contracts. The Seek/Close snapshot retained 167 entries / 56 contracts. The loose-file dtaRead snapshot retained **168 scoped entries / 57 contracts; 0/12 strict full-system gates**. The archive-read extension retained those counts and its 68 original-code READ observations and 32 SEEK context rows. The prior bounded native archive registration/Open milestone retained **176 scoped entries / 58 contracts**, with 24 native opens among 81 log rows and 976 compared bytes. The later bounded PCM16 mono/stereo WAV milestone reached **177 scoped entries / 59 contracts**. Native Write, generic non16 decoding, other channels, full DLL parity and app audio playback integration remain open. Existing creation/removal/Close composition adds no duplicate entry. Current admitted plain-object CPU loaded-resource ownership is distinct from full native texture Create/worker/upload, whole-device shutdown, derived LOD/GPU adapters and the first mission, which remain open. See [coverage](../COVERAGE.md) and [progress](PROGRESS.md).

| Reference | Audited revision | Useful evidence |
| --- | --- | --- |
| [LS3D-SceneKit](https://github.com/alex-alex/LS3D-SceneKit) | [`8fadd220`](https://github.com/alex-alex/LS3D-SceneKit/tree/8fadd2201f52936d998c66c47b0a0d37476c2ce5) | 4DS skin/morph layouts, 5DS channels, scene light fields and TCK/DAT sidecars. |
| [LS3D_FaceAnim](https://github.com/kirill-mapper/LS3D_FaceAnim) | [`a25706b2`](https://github.com/kirill-mapper/LS3D_FaceAnim/tree/a25706b29970887c6d0ef2ab34f22c262f9d6ebd) | Six facial parameters, ten regional morph weights and editor import/export behavior. |
| [mafia-re](https://github.com/Marvisak/mafia-re) | Observed [`9b793967`](https://github.com/Marvisak/mafia-re/tree/9b793967e83e6ce68bfb748a9fb8b7c04e54cbf7); behavior pin [`86a7b15c`](https://github.com/Marvisak/mafia-re/tree/86a7b15cbfa002bc6222e5542671a81988472882) | Original-address anchors, resource lifetime, factories, materials, visibility, collision, animation and explicit PC/PS2 differences. |
| [MafiaUnity](https://github.com/MafiaHub/MafiaUnity) | [`0d7a12f0`, develop](https://github.com/MafiaHub/MafiaUnity/tree/0d7a12f03ce4385173377c3e9f457dcc2e891f24) | Data readers, mission scene assembly and presentation adapters; prototype mechanics require separate native verification. |

**Decision:** use mafia-re to locate original engine algorithms and dependencies; use MafiaUnity to identify data-to-world integration gaps. Preserve the existing native-backed Rust contracts. Neither project supplies a completed original Game.exe runtime or first mission to transplant.

The stream follow-up confirms upstream `0c44be32` is unchanged; rw_data source also matches behavior pin `86a7b15`. Original exported `dtaRead` (RVA `0x25c0`) now has a bounded loose-file branch contract: **168/57**, including the prior Seek/Close entries. Its 27 Wine10/QEMU READ observations exclude archive decoding, native Open and whole-function parity. Snapshot/cache intake is a host adapter. Upstream Lock/Unlock and leak-detector names remain guesses; PS2 symbols do not establish PC member layouts.

The fresh source audit retains upstream `0c44be32`: its README marks Game.exe as started and `WinMain.cpp` implements a startup-directory wrapper at Game RVA `0x166160`, while `Main.cpp` remains a return-zero stub at `0x1bebe0`. No actor/script/mission lookup implementation was found. The inspected LS3DF frame-search dependencies remain source leads, without runtime equivalence or new Rust credit. The real-file host facade at `f04cc43` was a library-only step. At `b856d76`, existing Installation menu/static scene callers now use the selected native archive provider, retaining 177/59 and zero new function credit. Its exact-name/row/owner route bypasses the `dtaOpen` wrapper; AddRef/Release/destructor, Seek and app audio use are not demonstrated. That Installation batch reused primary REA evidence, with no fresh analysis, original execution or new upstream fetch. Its validation remains a historical receipt.

At `5b28728`, Rust C_car construction now feeds bounded primary Init, explicit cleanup and a real initialized-car inspection mode. The accepted scope is **272 entries / 93 contracts**, **21 static_inspection / 72 not_integrated**, strict **0/12**. Whole-function coverage remains N/A.

Actual PS2 linker evidence corroborates C_car::C_car, C_car::Init, C_car::GameInit and C_car_ex::Init; complete PC primary bodies and virtual slots establish their PC correspondences. Native comparisons cover the constructor and admitted primary Init/destructor paths only; GameInit and the derived Init wrapper remain static context without execution claims. The observed mission loader at 0x541030 calls CreateActor at 0x544b02, then primary virtual Init +0x48 at 0x544b12, tests AL and adds the actor on success. This static call-chain evidence adds no complete factory/loader or mission credit. Indexed alpha fragment comparisons establish palette-zero RGB equality for flags 8 and destination format 21; the host 0.5 cutoff adds no native TextureCreate/renderer entry credit.

Latest retained mafia-re source-only observation is `9b793967`; behavior remains pinned to `86a7b15c`. The observation reports six new upstream commits and source changes in math, rendering, texture and build files. No upstream build, original-instruction execution or Rust behavior credit follows from this source audit.

Actual authored MISE01 remains FINDACTOR/UnsupportedWorldBackend at PC 3 / source line 4; no later line executes. All six first-mission gates remain open. No runtime gameplay integration or driving mode is established.

Recover actual C_car::GameInit at 0x41ed00 separately from the observed mission CreateActor → primary Init → AddActor route. Complete Scene viewport/weather/shared Snow construction beyond 0x48e29 and the thirteen actual Scene Close effects with their correct resource identities, then original File Open/reload and mission-world ownership. Source-parameter flags 2/audio remains unavailable.

Historical replay busy/setter and `RECWAITFOREND` evidence remains recorded in [PROGRESS.md](PROGRESS.md) and the immutable replay observation. The pinned/current upstream trees and PS2 linker replay names establish no original PC cinematic-owner correspondence.

## Rust organization and original names

Ownership code uses `lhr_engine::ls3df::{c_str, itexture, i3d_object, texture_manager}` while physical source paths and existing Cargo packages stay unchanged. `I3dObjectResources` is a bounded ownership projection. `Lod` follows actual PC `lod::Create()` diagnostic bytes; `I3D_object` names have original PS2 symbols with independently researched PC correspondence. Member names retain their original-message, source or research provenance; a source spelling alone is not an original symbol.

| Original boundary | Current Rust organization and limit |
| --- | --- |
| LS3DF.dll | Recovered engine/resource components in `lhr-engine`; logical `ls3df::{c_str, itexture, i3d_object, texture_manager}` modules. `lhr-render` supplies wgpu presentation, without full IGraph/Direct3D or audio-library parity. |
| rw_data.dll | Bounded archive/provider behavior in `lhr-assets`; no complete DLL parity. |
| Game.exe | Scoped gameplay policies and the host application share `lhr-engine`; their origin remains separate from LS3DF. |
| IJoy.dll | Host winit input in `lhr-engine` is an adapter, without original input-DLL parity. |
| Setup.exe | Selected shared configuration in `lhr-engine::original_config`; no original Setup executable. |

`lhr-assets` also parses LS3DF/Game formats, so package placement does not identify an original loader or VM. mafia-re combines I3D, IGraph and sound in one LS3DF build target; its newer I3D subfolders are upstream organization, not recovered original packages. The behavior reference and retained address correspondence stay pinned to `86a7b15c`. The latest retained source observation is `9b793967`; its I3D subfolders remain upstream organization rather than recovered original packages. Observing newer source adds no Rust implementation or runtime credit.

## LS3D-SceneKit

The [skin reader](https://github.com/alex-alex/LS3D-SceneKit/blob/8fadd2201f52936d998c66c47b0a0d37476c2ce5/Mafia/Models/ModelMesh.swift#L47-L158) provides leads for inverse bind matrices, bone indexes, rigid/blended counts and two-influence weights. Independent PC reconstruction now admits bounded visual-type-2 skin records, Joint hierarchies, dense binding, eager bounds and CPU deformation. One original `civil02` first-LOD pipeline matches 3,124 u32 fields against original execution; 80 of 82 strict models produce static first-LOD poses, totaling 37,948 vertices. Two stop at numeric-domain checks. The [morph reader](https://github.com/alex-alex/LS3D-SceneKit/blob/8fadd2201f52936d998c66c47b0a0d37476c2ce5/Mafia/Models/ModelMorph.swift#L14-L119) provided leads for visual-type-3 loading, vertex links and per-LOD payloads. Independent REA/original-code reconstruction now loads the actual Tommy morph/Target model and renders bounded skeletal pose samples; eight-slot controller ownership and sequential playback with one live unit-weight slot are now implemented in bounded contracts. Full resource lifetime, weighted multiple slots and event integration remain open.

Several presentation choices cannot establish original behavior. Missing parents fall back to the root, only the first LOD is rendered, and some recognized visual types have no payload implementation. Material animation and lightmaps are skipped. [Scene lighting](https://github.com/alex-alex/LS3D-SceneKit/blob/8fadd2201f52936d998c66c47b0a0d37476c2ce5/Mafia/Scene/Scene.swift#L5027-L5062) uses tuned intensity scales. Our current unlit renderer also remains approximate; original driver, material, sector and visibility contracts are still required.

A specific incompatibility is [5DS interpolation](https://github.com/alex-alex/LS3D-SceneKit/blob/8fadd2201f52936d998c66c47b0a0d37476c2ce5/Mafia/Animations/AnimationLoader.swift#L387-L420): SceneKit uses smoothstep, while our original-code comparisons establish linear position interpolation in the supported domain. Keep that verified path. Bounded original rotation evaluation and sequential playback are now implemented; scale, notes, TCK root motion and audio-linked facial timing still require further recovery. No Apple application execution or SceneKit dependency was introduced by this audit.

## LS3D_FaceAnim

The complete two-file repository and both commits were inspected. Its [format description](https://github.com/kirill-mapper/LS3D_FaceAnim/blob/a25706b29970887c6d0ef2ab34f22c262f9d6ebd/README.md#L37-L58) and validator describe two header words followed by eight-byte records: six parameters and two trailing bytes. Import discards the trailing bytes; export writes `1,1` and canonicalizes the second header word. A future Rust reader should preserve all raw fields and validate lengths rather than assume those bytes are irrelevant.

The [weight equations](https://github.com/kirill-mapper/LS3D_FaceAnim/blob/a25706b29970887c6d0ef2ab34f22c262f9d6ebd/LS3D_FaceAnim.ms#L436-L503) map six bytes into ten weights across three regions. Each region sums to one; the editor multiplies weights by 100 for Max Morpher. This is a useful test reference, but native operation order and rounding remain unverified. The tool supplies neither joint/topology loading nor model compatibility checks beyond existing morph channels.

Mafia's 25 FPS and H&D2's 50 FPS are manual editor settings, not an encoded timing contract. New controllers default to Bezier; optional Step changes interpolation. Retained PC REA identifies candidate file/audio/state processing with 40 ms timing, but incomplete float recovery still needs instruction analysis and original-call probes. No Max execution, original facial playback comparison or H&D2/PS2 compatibility was established.

## mafia-re

This section retains the earlier pin and 2026-10-23 observations; the current
`109954d6` source refresh is recorded above.

The historical pinned [LS3DF card](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/docs/progress/LS3DF.svg) reports 97.60%, 1,350/1,361 source-associated functions and 98.39% average accuracy. The [current card](https://github.com/Marvisak/mafia-re/blob/0c44be32dd152c9722bbd8c2c8e18f112a279c04/docs/progress/LS3DF.svg) reports **98.46%**, the same **1,350/1,361** and **99.27% average accuracy**. This is improved instruction matching, with no increase in source-associated function count; average accuracy is not the percentage of exactly matching functions. These are upstream comparison results, not a playable-game percentage or Rust validation.

Game.exe now has a build target and one [WinMain implementation](https://github.com/Marvisak/mafia-re/blob/0c44be32dd152c9722bbd8c2c8e18f112a279c04/game/WinMain.cpp); its current card reports 1/5,037, 0.02% progress and 100% accuracy for that single function. The [main body](https://github.com/Marvisak/mafia-re/blob/0c44be32dd152c9722bbd8c2c8e18f112a279c04/game/Main.cpp) remains an explicit stub returning zero. No original menu or mission implementation follows from the bootstrap. A separate LS3DF 1.1 target reports 1,333/1,355, 95.12% progress and 96.69% average accuracy; it shares source with version macros and is not additive to the 1.2 count. Setup gains original-release and Steam language variants. These committed reports were inspected, not regenerated.

The [progress generator](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/tools/progress.py#L97-L143) uses reccmp's **effective per-function score**, not a byte count or runtime test rate. In pinned reccmp 0.1.7, an [effective match scores 1](https://github.com/isledecomp/reccmp/blob/2f6d84fc587efd1ff6453a17696a4951bcd09bd9/reccmp/compare/report.py#L53-L60); otherwise it contributes its raw comparison score. The [aggregate](https://github.com/isledecomp/reccmp/blob/2f6d84fc587efd1ff6453a17696a4951bcd09bd9/reccmp/compare/report.py#L160-L176) excludes stubs, with library/ignored entities filtered by mafia-re. If `M` is the eligible matched count, `S` its summed effective score and `D` the larger configured/known count, the bar is `S/D`, implemented is `M/D`, and accuracy is `S/M`. Functions have equal weight regardless of size or gameplay importance. Keep our Rust implementation and native-validation metrics separate; no upstream cards were regenerated here.

The [configured LS3DF and rw_data hashes](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/reccmp-project.yml#L1-L34) match our supplied binaries; Setup targets differ. This makes source addresses useful research leads, not new verification of each body. Written implementations retain nonmatching caveats, including [LitObject](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/I3d_lit_object.cpp#L1-L18) and [Joint matrix evaluation](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/I3D.h#L2870-L2902).

### Concrete engine entry points

Addresses below are **upstream PC virtual-address leads**, not newly admitted native function boundaries. LS3DF's preferred image base is `0x10000000`; subtract it for an RVA. Names retain upstream provenance until independently checked against symbols/instructions.

| Dependency | Pinned implementation and address lead | Useful next Rust work |
| --- | --- | --- |
| Provider and stream lifetime | [`dtaOpen`, rw_data VA `0x100018a0`](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/rw_data/rw_data.cpp#L221-L247) | Selected Installation archive open/read/close now uses the native facade. Recover remaining real callers and ownership, especially production texture-worker/surface effects; preserve exact selected names and fail-stop errors. |
| Concrete resources and geometry | [`LoadImage` / `LoadMeshImage`, LS3DF VA `0x10060cb0` / `0x10061120`](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/load4ds.cpp#L42-L286), [driver factories](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/I3d_driver.cpp#L1280-L1457) | Frame/visual construction, shared meshes, all admitted LODs, material references, registration and failed-load state. |
| Persistent model/scene lifetime | [Model `Open`, VA `0x100335a0`](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/I3d_model.cpp#L26-L89), [scene `Open`, VA `0x1004a300`](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/I3d_scene.cpp#L509-L563), [`LoadHierarchy`, VA `0x100603e0`](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/Load.cpp#L67-L190) | Real resource backends, reopen/close, derived destruction, hierarchy ownership, callback and failure ordering. |
| Material resources and execution | [`I3D_material::LoadImage`, VA `0x1002bd50`; animated loader `0x1002c000`](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/I3d_material.cpp#L409-L613) | Color-key/separate-alpha variants, texture sequencing, mipmaps and original material-to-draw state. |
| Coordinate space and lit draws | [`I3D_object::SetSpace`, VA `0x100378b0`](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/I3d_object.cpp#L437-L491), [LitObject `m_AddFGroups`, VA `0x10027880`](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/I3d_lit_object.cpp#L1501-L1604) | Shared-mesh detachment, all-LOD transforms, bounds, sector/lighting caches and buffer lifetime. Retain current unsupported-state rejection until verified. |
| Visibility and scheduling | [`ProcessPortals`, VA `0x10045700`](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/I3d_portal.cpp#L1734-L1778), [`Render`, VA `0x10062030`](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/render.cpp#L449-L484) | Sector membership, camera-driven portals/occluders, LOD and draw ordering; preserve sound/update dependencies. |
| Geometry-backed queries | [`TestColGeo`, VA `0x1000dec0`](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/i3d_collision.cpp#L795-L901), [`TestColSphere`, VA `0x10011d60`](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/i3d_collision.cpp#L1312-L1397) | Hierarchy/bounds traversal, filters, callbacks and nearest-hit state. KLZ loading and game collision remain separate dependencies. |
| Animation resources and remaining evaluation | [`I3D_animation_set::Open`, VA `0x10004900`](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/I3d_animation.cpp#L60-L106), [`I3D_interpolator::Update`, VA `0x1001d320`](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/I3d_interpolator.cpp#L113-L320) | Extend existing one-slot playback with scale, ordered weighted slots, pose/notes and real resource/event lifetime. |

The existing audited LS3DF union contains 764/1,255 pinned upstream candidate addresses as exact entries; 491 still require correspondence. These correspondence counts remain historical against `86a7b15c`; the current source refresh does not recount them. PC `GetVertexJoint` at RVA `0x54dd0` already has 2,040 admitted native comparisons. These retained address/case counts are discovery and validation evidence, not semantic coverage of every caller or new credit from this audit.

## MafiaUnity and DataFiles.md

The requested [DataFiles.md](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Docs/DataFiles.md#L1-L37) explicitly describes work in progress. It is a useful dependency inventory, not proof that every listed format runs. The audited develop commit dates to 2020-06-11.

### Data loading versus connected runtime

[`MissionManager.LoadMission`](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Core/Managers/MissionManager.cs#L33-L88) creates a root and reference maps, loads `scene.4ds` → optional `cache.bin` → `scene2.bin` → optional `tree.klz`, applies `MissionHacks`, then invokes its loaded callback. This exposes useful relationships between resources. **It is Unity's construction order, not recovered Game.exe boot order.** Its unload path also highlights the need for explicit mission/resource lifetime.

| Data | Actual inspected implementation | Reuse decision relative to current Rust |
| --- | --- | --- |
| `scene.4ds`, `scene2.bin` | Model hierarchy, named scene objects, patches, partial physical/door/light metadata | Existing Rust readers already handle bounded records and original script text. Extend generic named-reference/actor ownership and missing verified metadata rather than replace parsers. |
| `cache.bin` | Loads groups/instances; bounds remain opaque and only the first scale is applied | Rust already retains bounds, flags and secondary scale. Recover live residency/culling and shared instances. |
| `tree.klz` | Six shape arrays and links; spatial grid explicitly TODO | Valuable missing-format lead, followed by native grid/query/material semantics. No faithful collision implementation supplied. |
| `check.bin` | Standalone point/link reader; no mission loader or pathfinding caller found | Candidate graph intake, not AI/traffic. Preserve unknown fields until native interpretation. |
| `road.bin`, `car_table.bin`, `clusters.seg`, `effects.bin` | Listed in documentation; no parser or mission caller found in this tree | Original-binary research still required. |
| TCK | Parser exists; [player call is commented out](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Rendering/ModelAnimationPlayer.cs#L80-L100) | No root-motion runtime credit. |

The [KLZ array reads](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/API/MafiaFormats/KLZLoader.cs#L291-L342) give concrete layout leads, including an array order different from the header count order. But [face/cylinder conversion](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Internal/Generators/CityGenerator.cs#L69-L144) substitutes entire render mesh colliders, discarding triangle selection or cylinder geometry and surface properties. Preserve actual collision shapes and links when reconstructing Rust; visual meshes alone do not establish walkable surfaces or vehicle clearance. [CheckBIN's reader](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/API/MafiaFormats/CheckBIN.cs#L59-L113) suggests 30-byte points and 8-byte links, but neither graph directionality nor the unknown link float's meaning is established.

The [filesystem](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Core/FileSystem.cs#L196-L235) supports mod/loose/DTA lookup, but its automatic archive-mount call is commented out. Its precedence and duplicate handling differ from our bounded native-backed provider path; do not adopt them as original policy. DTA decompression and cache/scene/5DS/BMP/TGA intake already exist in Rust within explicit limits.

### Materials and presentation

The [face-group generator](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Internal/Generators/ModelGenerator.cs#L329-L469) is a useful checklist for keyed/transparent materials and texture sequences. Our renderer currently samples an opaque, unlit texture although the asset layer retains richer material metadata. The useful next abstraction is an original-backed draw/material descriptor with separate texture slots, alpha interpretation, sidedness, depth/sort state and cache identity.

Do not copy Unity's constants or shortcuts as native semantics: its [texture cache](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Internal/Generators/ModelGenerator.cs#L484-L545) keys only by path despite differing alpha decode options; the generator assigns a separate alpha map over diffuse `_MainTex`; its [texture player](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Rendering/TextureAnimationPlayer.cs#L9-L39) advances at most one frame and discards overshoot. These are source-level risks, not reproduced runtime failures in this audit.

Lightmap enum/shader declarations have no connected resource-loading path, and parsed portals have no visibility consumer in this tree. Sector [fog/ambient selection](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Utils/CameraExtensions.cs#L8-L138) uses AABBs, hierarchy and tuned multipliers. Keep native sector/lighting research separate. Likewise, the [animation player](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Rendering/ModelAnimationPlayer.cs#L278-L450) advances key-array indices at 25 Hz without using stored sparse timestamps for sampling. Our original-backed interpolation, controller ownership and skin arithmetic should remain authoritative within their admitted domains.

### Gameplay boundary

The [base startup](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Game/GameMain.cs#L57-L75) explicitly leaves the original menu TODO. [C# mod compilation](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Core/Modding/Mod.cs#L48-L95) executes `ScriptMain`; it is not Mafia's original mission VM. The [object factory path](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Game/ObjectDefinition.cs#L107-L150) installs Unity Rigidbody/capsule and idle character controllers, with no car special-type branch. No connected original campaign, traffic, police or mission-objective runtime was found.

Useful architecture leads are the mission lifetime boundary, staged named-object resolution, actor dispatch and data → interaction → visual/audio dependencies. Original ordering, physics, state transitions and constants must still come from REA and original execution. Keep our existing bounded script control-flow/scheduler contracts and bind their missing world effects; replacing them with C# mods would not advance original mission compatibility.

## Next general-engine work

The next bounded milestone is **the real file-backed IGraph → ITexture path
needed by the Scene constructor**, followed by its shared weather ownership.
Whole managed-texture conversion is a separate dependency. The current
`TextureResources::create_texture` creates blank constructor state;
`install_texture` and `register_cache` explicitly admit host state. They do not
perform filename lookup, image creation or factory cache acquisition.

1. **Immediate texture creation and identity.** Recover the filename/cache
   factory and reached flags-2 Create path using REA, actual PS2 naming evidence
   and original-instruction comparisons. Reuse the accepted ITexture/string/
   Close/Release ownership with real storage effects. Check source BPP and alpha,
   cache hit/miss and name/flag distinctions, returned references, failure output
   and actual graphics/pixel/string cleanup. Use synthetic format fixtures and
   original images as regression inputs, without a Snow-specific algorithm.
   A bounded canonical owner can precede conversion of other model textures;
   it must not claim that their separate CPU/managed lifetimes are now unified.
2. **Scene construction and shared teardown.** Bind viewport dimensions,
   weather reset, conditional shared Snow creation and sound defaults in the
   recovered order. Two admitted scenes must share one Snow identity: removing
   the first preserves it; the native last-scene condition releases the shared
   reference, while an external retained reference survives. Close does not
   own Snow release. Preserve the original unchecked Create result rather than
   inventing constructor rollback. Driver registration, optional listener,
   remaining Close effects and complete destructor/Open/reload each require
   their actual owners before receiving completion credit. Existing VB → IB
   cleanup remains bound; unavailable effects must still fail explicitly.
3. **Original mission ownership and scheduling.** Use the new `main` source
   map to research enclosing Mission Open/Close, Game initialization, actor
   factories/registration and the original player/camera loop. Then bind the
   existing script scheduler/world effects, replay and objectives. MafiaUnity
   and OpenMF provide name/patch regression ideas, not an original actor order.
   The authored MISE01 program probe remains at FINDACTOR/UnsupportedWorldBackend;
   application mission scripting is still unavailable.

Repeated resource create/release and scene construction/destruction must return
real allocations to the admitted baseline, preserving external survivors and
native failure prefixes. Each code milestone still needs binary-keyed contracts,
independent Rust implementation, appropriate original execution evidence and
two sequential independent reviews. Material/sector/portal rendering and
foreign dynamic actors remain parallel research dependencies. This source audit
changes no coverage count, graphical progress bar or first-mission gate.

## Direct PS2 symbol crosscheck

The supplied USA `SLUS_206.71` executable (SHA-256 `adf4944c5cdac23fcc4e9af9ef3ec1ba6234497e57cec2af2ff1a7a15c62a46f`) contains original function/global linker symbols, but no DWARF or STABS type/member information. mafia-re also distinguishes original symbols from guessed field names. The earlier batch checked 27 complete symbol ranges. The earlier singlemesh batch captured 22 related ranges plus two supplemental ranges; the captures overlap earlier work and are not an additive unique-function count. All are static evidence against unchanged original bytes, without PS2 execution credit.

| Original PS2 name | PS2 address | PC correspondence and evidence limit |
| --- | --- | --- |
| `I3D_joint::UpdateV` | `0x42b750` | LS3DF RVA `0x1ee30`; matching invalidation role, different masks and offsets. |
| `I3D_joint::LoadImage` | `0x42b850` | LS3DF RVA `0x1ef10`; PS2 reads a four-byte suffix, PC reads 68 bytes. |
| `I3D_singlemesh::DBuild` | `0x456f00` | LS3DF RVA `0x54c20`; PS2 zeroes joint-pointer holes, actual PC instructions leave unmatched slots unwritten. |
| `I3D_joint::GetJointMatrix` | `0x456cc0` | LS3DF RVA `0x56ba0`; matching cache/traversal and multiplication order, different storage and local-matrix dependencies. |
| `I3D_visual::UpdateV` | `0x45bf70` | LS3DF RVA `0x5a390`; inherited by singlemesh on both platforms, with different invalidation masks. |
| `GetAnimIndex(unsigned int)` | `0x117a80` | Game RVA `0x8760`; instruction-level agreement on unsigned keys, duplicate midpoint selection and null/miss behavior. |
| `C_human::Do_Crouched` | `0x17b2a0` | Game RVA `0x17f8a0`; structural correspondence, with additional PS2 collision-shape stores. |
| `C_human::Do_Shoot` | `0x1802f0` | Game RVA `0x183590`; matched action-kind branches and animation resource references. |
| `C_human::PersonAnim` | `0x16cf10` | Game RVA `0x173e50`; matched code flags, lookup and resource-binding structure. |

These are original PS2 names with separately established PC correspondences, not recovered PC symbol tables. Member labels remain research names. PC model animation has eight channels; PS2 has three. PS2 skin assignments use two bytes per vertex and a secondary weight divided by 256; PC uses per-joint float weights for the primary influence. PS2 CPU position output has stride 16; PC position/normal output has stride 24. PS2 joint discovery returns early at nested singlemesh/singlemorph visuals, stopping the current sibling scan; PC keeps traversing. Inherited visual UpdateV clears PS2 mask `0x140000` versus PC `0x1c0000`. These differences remain explicit alongside the array-hole and Joint-layout differences above.

Fresh static research associates the original PS2 `I3D_mesh_object::UpdateBoundVolume` symbol (VA `0x43c750`) with PC RVA `0x31400`. PS2 uses three LOD slots versus ten on PC, ref/count/slots offsets `0x30/0x34/0x38` versus `0x04/0x08/0x0c`, and VU/EE arithmetic rather than PC x87. These are original-symbol and instruction findings only, without PS2 runtime equivalence.

Fresh original PS2 symbols identify the IB/VB manager methods, with separately researched PC associations. PS2 manager allocation creates whole buffers, whereas PC pools shared blocks; PS2 VB allocation also lacks the PC FVF argument. Both PS2 `UnlockAll` bodies (VAs `0x479870` and `0x479ba0`) contain only `jr ra; nop`, unlike the PC lock-list cleanup. PC per-buffer shared-block allocation/free names remain descriptive research names, without matching original PS2 symbols. These instruction findings are static only, with no PS2 runtime equivalence.

Fresh static object research confirms a separate PC ten-generated-LOD cleanup layer. The original PS2 object teardown is split between `__dt__10I3D_objectFv` (VA `0x4243f0`) and `BogusDestructor__10I3D_objectFv` (`0x43f9c0`); neither captured body performs that PC loop. PS2 `SetMesh` (`0x43fa00`) lacks the PC keep-compatible/delete-ten path. The plain `Lod` name comes from PC diagnostic bytes, without a corresponding PS2 linker-name claim. These are static platform differences, with no PS2 execution evidence.

Fresh static texture analysis maps actual PS2 `ITexture` constructor/destructor, `Release`, `Close` and `Unlock` symbols to independently researched PC entries; `C_str` assignment also has an original PS2 symbol. PS2 uses a different texture layout and deleting-destructor convention. Its `Close` frees raw filename allocations, whereas PC assigns reference-counted empty `C_str` representations; surface Unlock uses vtable offset `0x14` on PS2 versus `0x28` on PC. These observations add no PS2 runtime validation. Representation-helper names remain descriptive/source-assisted, and only `m_RefCount` is supported as an original texture member spelling by the PC diagnostic.

The managed-removal helper at PC RVA `0x7aee0` retains a descriptive upstream name; no original PS2 counterpart is established. Its finite record/queue/pool cleanup and Close composition do not reconstruct creation, worker scheduling or pool shutdown. The existing PS2 caller difference is retained static context, without new PS2 execution evidence.

The creation helper at PC RVA `0x7ad20` is named `AddTextureToTextureManager` by its original PC diagnostic bytes. This supports the function spelling, without establishing a linker signature, original record/member/global names or a PS2 counterpart. Bounded record/filename/registry ownership is distinct from image loading or worker execution.

The producer at PC RVA `0x7a910` uses the pinned reference working name `ITexture::Request`; no original symbol or PS2 correspondence is established. Fresh REA/Ghidra confirms its finite producer body and data-table reference, without a recovered direct caller. Queue production does not load or upload textures.

The standalone pool-unload helper at PC RVA `0x7afe0` uses the pinned reference working name `UnloadManagedTextures`, without a recovered original PC/PS2 symbol. Fresh actual REA confirms its finite body and sole resolved direct caller; the call precedes the caller's scene-argument load. This does not establish whole SceneClose integration. REA reports the pool datatype as `undefined4`; pool offsets remain assembly evidence, without a recovered struct claim.

Bounded standalone managed texture Restore now reconstructs admitted system/default pools and reserves the registry through controlled device/storage effects. It composes the accepted Release dependency, selects raw format once, uses returned descriptor Size with signed budgets and wrapping memory arithmetic, grows arrays in 1,024-slot steps and preserves registry entries. RestoreManagedTextures is a pinned mafia-re working name without recovered original PC/PS2 symbol. Admitted plain-object loaded resources now connect to CPU ModelArena ownership. Full native texture Create/worker/upload, real GPU/device implementation, whole-device shutdown, derived lit/skin/morph resources and application/mission integration remain open; these standalone resource contracts establish no original-renderer parity.

The bounded pool-release helper at PC RVA `0x7a610` uses the pinned reference working name `ReleaseManagedTextures`, without recovered original PC/PS2 symbol. Fresh REA confirms per-pool system/slot release and array/header/memory cleanup after the locked unload prefix. Controlled native probes validate admitted state with seeded Rust intake; they do not establish real Direct3D destruction, Restore or whole-device shutdown.

Current upstream adds `tools/ps2fn.py`, which reads actual ELF symbol names and sizes and resolves disassembly call targets. Its generic Capstone MIPS decoder leaves unsupported R5900 instructions as raw bytes; this tool supplies neither member/type layouts nor new platform-equivalence evidence. The PC managed-texture section explicitly identifies its manager names as guessed and the subsystem as PC-only. `RenderThreadProc` at PC RVA `0x7a9b0` remains a working source name with an explicit instruction-matching caveat, without an established PS2 counterpart.

REA currently rejects the Emotion Engine R5900 variant with `architecture_unsupported`. The PS2 crosscheck used the retained dedicated EE Ghidra backend in read-only mode; PC REA and original-code comparisons remain separate. Future contracts record native binary/address, exact symbol, name origin, mapping evidence, Rust symbol and validation independently. This crosscheck adds no completed-function credit.

The native Open follow-up uses fresh REA evidence for eight bounded canonical entries and PS2 symbols for original names. Ghidra did not independently decompile UnlockPack at RVA `0x13b0` or Release at `0x1850`; accepted raw instruction boundaries and independent final-source reviews cover those limits. Static inventory counts are unchanged. Native stored-fixture comparisons, scripted Rust effect cases and the supplemental Rust-only installation probe remain distinct. The subsequent bounded WAV codec is implemented. Selected Installation menu/static scene archive routing is now connected; worker routing and app audio playback remain open.

The WAV milestone uses eight fresh PC REA results. Original PS2 linker symbols identify `WavDecompression` (VA `0x003ded50`) and `wav_table` (`0x0049ead0`); all 896 coefficients match the PC table exactly. This supports naming and numeric constants, without PS2 runtime or whole-platform semantic equivalence. Bounded PCM16 mono/stereo decoding has separate native archive/Read comparisons; generic non16 LZ, other channel configurations, Write and app audio playback integration remain open.
``````````

<a id="progress"></a>

## docs/PROGRESS.md

``````````markdown
# Progress

## Current milestone — 2026-10-26

The staged source-based merge adds connected loader/image fixes, live material
and model LOD selection, authored view orientation, audio lifecycle,
archive/write IO and data-language selection. The unconstructed Lit branch is
supported; saved lightmaps and the scene `LMAP` payload association remain open.

Accepted code `a4dee6d` passes the required gate and **1310 Rust tests
in both debug and release**, including both unchanged x87 tests without
exclusions; 9 Python boundary and 47 progress tests pass. Seven opt-in GPU
checks pass on Mesa llvmpipe Vulkan. New standalone W/A/D/S/Space, three
reloads and Escape finish with four cleanups and seven zero CPU counters.

Standalone inspection/driving remains separate from a playable first mission.
All six first-mission gates stay open. Accepted work remains **311 scoped
entries / 114 contracts**, strict **0/12**; no new function or native-runtime
credit is added.

[Coverage](../COVERAGE.md) · [goals](GOALS.md) ·
[stage scope and validation](implementation/mafia-re-remediation.md) ·
[preceding current narrative](history/2026-10-26-pr-merge-validation.md).

## Earlier milestones

[Previous driver/railway Source-v2 milestone](history/2026-10-25-driver-railway-source-v2.md)
retains its original 273-file/1113-test validation and separate cohorts.

[Previous texture/Scene Source-v5 milestone](history/2026-10-25-texture-scene-source-v5.md)
retains its original 270-file/1093-test source and native/UI scopes.

The [verbatim narrative through baseline `0da90fb`](history/2026-10-25-progress-narrative.md)
retains the weather/default workers, standalone driving, resource ownership,
animation, archive and script milestones with their original source/proof scope.
The [reference audit](REFERENCE_PROJECTS.md) remains the technical caller/name map.
Machine observations and immutable snapshots remain in [progress.json](progress.json).

<details>
<summary>Generated static inventory, accepted ports and ledger history</summary>

<!-- generated-function-progress:start -->
As of 2026-10-25 · snapshot `RUST-SCOPED-PORTS-20261025-47`.

| Original binary | Static retained pseudocode | Denominator basis | Full contracts | Full Rust ports | Differential runtime verified |
| --- | ---: | --- | ---: | ---: | ---: |
| Game.exe | 4784 / 4,914 (97.35%) | Explicit profile union | N/A | N/A | N/A |
| rw_data.dll | 176 / 234 (75.21%) | Explicit profile union | N/A | N/A | N/A |
| LS3DF.dll | 1555 / 1,714 (90.72%) | Explicit profile union | N/A | N/A | N/A |
| Setup.exe (supplied configuration) | 172 / 172 (100.00%) | In-image non-thunk procedures | N/A | N/A | N/A |

Strict full-system gate: **0/12**. Binary-level static counts use the named denominator basis and do not combine into a game-completion percentage. Unknown denominators remain N/A. 1 supplied configuration Setup inventory uses 172 in-image non-thunk procedures. Separately, archive installer bootstrap static inventory: **376 / 376 (100.00%)**; this distribution target is not combined with engine-library counts. The campaign is not playable.

Accepted scoped Rust ports: **311 entries across 114 reviewed contracts**. These remain separate from retained pseudocode and whole-function counts.

| Rust contract | Source | Original entries | Native-derived cases | Runtime use | Verified scope |
| --- | --- | ---: | ---: | --- | --- |
| Clock reset and elapsed time | [crates/lhr-engine/src/clock.rs](../crates/lhr-engine/src/clock.rs) | 2 | 20 | Static inspection | Serialized shared u32 clock with the standard sampling/sleep backend; no arbitrary IGraph overrides or caller lifecycle. |
| Scene callback dispatch | [crates/lhr-engine/src/scene_tick.rs](../crates/lhr-engine/src/scene_tick.rs) | 1 | 13 | Static inspection | Immediate mutations and reentry in valid logical collections; animation and actor lifetime remain backend responsibilities. The static inspection uses the constructor-empty Scene path; active scene callbacks remain unbound. |
| Interpolator callback control | [crates/lhr-engine/src/interpolator.rs](../crates/lhr-engine/src/interpolator.rs) | 1 | 31 | Static inspection | Caller control: 23 callback-observer and 8 scalar projections; evaluator math, poses and full note streams remain backend responsibilities. |
| Game frame-delta modifier | [crates/lhr-engine/src/frame_delta.rs](../crates/lhr-engine/src/frame_delta.rs) | 1 | 22576 | Static inspection | Typed state with masked x87 precision and rounding; original-body comparison excludes FPU status flags, traps and gameplay integration. |
| Generic actor callback dispatch | [crates/lhr-engine/src/actor_tick.rs](../crates/lhr-engine/src/actor_tick.rs) | 1 | 26 | No integration recorded | Visibility and live virtual-call ordering with mutation and reentry; callback implementations, actor ownership and gameplay integration remain open. |
| Game frame coordinator dispatch | [crates/lhr-engine/src/frame_dispatch.rs](../crates/lhr-engine/src/frame_dispatch.rs) | 1 | 1509 | Static inspection | Bounded valid-storage coordinator control flow and arithmetic with synthetic callbacks; required helpers, mission initialization, object lifecycle and full Game integration remain dependencies. |
| LS3DF model tick caller dispatch | [crates/lhr-engine/src/model_tick.rs](../crates/lhr-engine/src/model_tick.rs) | 1 | 7 | Static inspection | Bounded valid model storage caller guard, IPol tick and live finalizer dispatch; the seven Rust checks project selected behaviors from an eleven-case native oracle, not a full eleven-case state differential. Evaluator, finalizer, pose and Game integration excluded. This receipt covers only the static null-IPol path. |
| Selected game-loop control | [crates/lhr-engine/src/selected_loop.rs](../crates/lhr-engine/src/selected_loop.rs) | 1 | 123 | No integration recorded | Bounded selected-loop caller control with explicit fallible callbacks; whole Game ancestry, host adapter integration and invalid native memory behavior remain excluded. |
| 5DS isolated position-channel intake and sampling | [crates/lhr-assets/src/animation.rs](../crates/lhr-assets/src/animation.rs) | 1 | 2684 | Static inspection | Isolated 5DS position-channel parser, selected reader and evaluator branch within the bounded data/numeric domain; no quaternion/scale math, model binding, IPol lifecycle, hierarchy, skinning or rendered animation parity. |
| Named script lookup callers | [crates/lhr-engine/src/named_script.rs](../crates/lhr-engine/src/named_script.rs) | 2 | 13 | No integration recorded | Mission/global lookup caller control against controlled dependencies; ASCII comparison is bounded, and VM/world/mission effects remain unimplemented. |
| Program reset/start caller | [crates/lhr-engine/src/named_script.rs](../crates/lhr-engine/src/named_script.rs) | 1 | 9 | No integration recorded | Synchronous reset/start caller state mutations before a controlled scheduler observer; full scheduler, VM and program/mission effects are not implemented. |
| LS3DF affine transform primitives | [crates/lhr-engine/src/native_transform.rs](../crates/lhr-engine/src/native_transform.rs) | 7 | 531 | No integration recorded | Seven shipped LS3DF PE exports within the per-call finite-magnitude guard and ordinary round-to-nearest host assumption. 531 accepted cases; one outside-domain case rejected. No all-f32/x87-environment, quaternion-from-matrix, inverse, hierarchy, rendering, or runtime-integration claim. |
| LS3DF quaternion normalization | [crates/lhr-engine/src/native_transform.rs](../crates/lhr-engine/src/native_transform.rs) | 1 | 809 | No integration recorded | S_quat::Normalize at original export RVA 0x30140 in the declared CW 0x003f magnitude-guarded domain; 809 native cases, zero component mismatches in debug and optimized comparisons. Caller must recheck normalized values before subsequent guarded primitives. |
| Shared original configuration behavior | [crates/lhr-engine/src/original_config.rs](../crates/lhr-engine/src/original_config.rs) | 6 | 1577 | No integration recorded | Six bounded shared configuration contracts: five supplied Setup.exe functions and Game.exe defaults. Native projections preserve 63-byte value behavior and selected callbacks/mode lists. No GUI, registry, device enumeration, DirectX creation, installer or whole Setup/Game integration claim. |
| Game script scheduler and bounded dispatcher control | [crates/lhr-engine/src/script_scheduler.rs](../crates/lhr-engine/src/script_scheduler.rs) | 2 | 440 | No integration recorded | Scheduler RVA 0x6ccb0 plus bounded base-dispatcher control at 0x6d3b0; 440 comparison cases and 789 ordered dispatches. This is not complete dispatcher/VM function coverage or full Game integration. |
| Game startup effect and resource behavior | [crates/lhr-engine/src/mission_effects.rs](../crates/lhr-engine/src/mission_effects.rs) | 5 | 6825 | No integration recorded | Five bounded Game.exe entries. Three independent fixture domains: 295 caller-state cases, 640 light traversal/vector cases, and 5,890 resource queries across 155 tables. These domain counts are not a claim of 6,825 full startup executions or complete mission/UI/runtime behavior. |
| LS3DF vector length and quaternion composition helpers | [crates/lhr-engine/src/native_transform.rs](../crates/lhr-engine/src/native_transform.rs) | 2 | 1063 | No integration recorded | Two internal LS3DF helpers named descriptively: vector_length at 0x2f9f0 and SQuat::compose at 0x2fa70. Dedicated fixture contains 532 length calls and 531 composition calls, 1,063 per-function calls rather than unique input rows. Complete original routines were compared on a physical CPU; separate R1 5,000-case paired and R2 1,200-case fresh physical corpora had zero debug/optimized bit mismatches. Declared finite scalar guards and x87 CW 0x003f apply. No ABI or FP-status equivalence, Make/axis-angle, normalization, or runtime integration claim. |
| LS3DF I3D frame transform core | [crates/lhr-engine/src/frame_transform.rs](../crates/lhr-engine/src/frame_transform.rs) | 8 | 7800 | Static inspection | Eight bounded frame entries; 7,800 native-derived cases retain the historical 7,798 and add two complete original SRT LinkTo/getter trajectories. The SRT corpus now has 180 complete cases and 76 mixed availability stops; the classification erratum preserves the old evidence. Normal-or-zero per-operation guards close 59 previously unavailable Rust source-frame worlds; two AXIS00 frames remain unsupported. A separate original taxi hierarchy comparison matches 66 world matrices/1,056 fields. Separate accepted physical primitive calls are 18,432 SetRot3, 13,960 row scaling, 12,945 affine products and 13,971 point transforms; these per-function counts and the Rust-only 1,953-frame replay are not added to row cases. Prior complete common LoadImage field projection, model/mesh/dummy updates and bound invalidation remain bounded; short reads, FF-enabled loading, sectors and derived resource propagation remain excluded. Dummy UpdateV stays an uncredited dependency. Static model graph and checked point rendering are integrated; flags-one application binding, general geometry loading and original renderer/gameplay parity remain unavailable. |
| LS3DF bounded quaternion Make | [crates/lhr-engine/src/native_transform.rs](../crates/lhr-engine/src/native_transform.rs) | 2 | 18123 | No integration recorded | Original Make entries in a guarded expanded domain: nonzero axis angles up to two radians and nondegenerate matrices whose two original factor angles are admitted. Portable outward intervals include documented reduced-domain x87 error and reject ambiguous f32 rounding; new PC24 intermediates remain ordinary. Physical comparisons match all 18,123 admitted Make calls from nonzero-axis, synthetic-matrix and source-SRT-derived matrix corpora; 4,892 other matrix attempts return explicit errors. Prior 12,900 zero/diagonal calls remain exact. Missing-row reconstruction and larger factor angles remain unavailable. Source-SRT inputs do not establish native loader parity; no new application animation binding. |
| LS3DF raw driver storage | [crates/lhr-engine/src/driver_state.rs](../crates/lhr-engine/src/driver_state.rs) | 2 | 245 | No integration recorded | Descriptive working names for valid-index reads and 49 storage-only write arms. 245 original setter calls with 15,435 getter checks match all 62,720 output bytes in debug and optimized Rust. Caller supplies the shared 63-word bank; no constructor defaults or renderer effects are assumed. Other write arms and invalid indices return explicit host errors; native invalid-index behavior is outside scope. No police or application integration. |
| LS3DF affine inverse | [crates/lhr-engine/src/native_matrix.rs](../crates/lhr-engine/src/native_matrix.rs) | 1 | 10423 | No integration recorded | Original S_matrix::Inverse with distinct source/output, finite normal-or-zero operands and guarded intermediates under PC24 round-to-nearest. Physical original code matches success flag and all 16 output fields for 10,423 accepted cases in debug and optimized Rust; 9,592 additional cases are explicitly unavailable. Preserves singular identity fallback and the original widened f32 cutoff. No alias, other FPU policy, FP-status or application integration claim. |
| LS3DF scale extraction and direction basis | [crates/lhr-engine/src/native_matrix.rs](../crates/lhr-engine/src/native_matrix.rs) | 2 | 21809 | No integration recorded | Original GetScale at RVA 0x2e3a0 and SetDir3 at RVA 0x2dcf0 with distinct operands under guarded PC24 round-to-nearest arithmetic. Physical originals match 11,104 admitted scale calls and 10,705 admitted direction calls; 10,239 other per-function attempts are explicit numeric or small-residual rejections. These are per-function calls from 16,024 paired input rows. GetScale ignores translation and the fourth column; SetDir3 preserves them and retains partial writes on error. Degenerate-up fallback, aliasing and other FPU policies remain unavailable. No original scene loading or application integration claim. |
| LS3DF parsed model ownership and frame materialization | [crates/lhr-engine/src/model_loading.rs](../crates/lhr-engine/src/model_loading.rs) | 8 | 1022 | Static inspection | Eight existing parsed constructor/common-field/owner/load projections retain their prior 1015 cases and add seven persistent ownership scenarios as a distinct cohort. Last-reference transfer/deletion now reaches admitted teardown; Close and destructors are counted separately. Parsed same-root reopen retains failed prefixes and invalidates stale playback. Real bedna00/taxi00 host tests retain old/new source-backed instances while excluding final visual release. Native file loading, filenames, factories, mesh/material/texture resources and whole-game lifetime remain unavailable. |
| Game bounded actor and frame cache consumption | [crates/lhr-engine/src/actor_registry.rs](../crates/lhr-engine/src/actor_registry.rs) | 2 | 1260 | No integration recorded | Two cache-consumption entries over existing records: reverse ASCII case-insensitive matching, actor-kind filter, owned-name release, swap-last removal and ordered required backend effects. Original 1,260 cache cases match Rust in debug and optimized builds; additional ASCII and floating-class corpora validate dependencies. Names use the declared ASCII domain. Backend actor state and type-specific effects remain required dependencies. No cache insertion, drain, actor construction, resource lookup or application integration. |
| Game raw police-manager initialization and reset | [crates/lhr-engine/src/police_state.rs](../crates/lhr-engine/src/police_state.rs) | 2 | 1756 | No integration recorded | Bounded constructor stores and logical reset match 1,756 original input states in debug and optimized Rust: 302,032 state bytes and 65,536 retained storage bytes checked. Existing bytes and the constructor marker are explicit inputs; construction does not assume a zero mode. Reset truncates four pools while preserving allocations, pointees, script arguments and other unknown state. Opaque pointer words are never dereferenced. No native heap/ABI, resource creation, police ON or application integration claim. |
| Game bounded player control and model-animation helpers | [crates/lhr-engine/src/player_control.rs](../crates/lhr-engine/src/player_control.rs) | 5 | 6284 | No integration recorded | Five bounded Game entries over existing actor/model storage: direct byte-valued unlock and callback-free lock paths, script boolean normalization and follow-up reset, raw direction/embedded animation resets, and eight model-animation calls with a live owner read before each call. Rust debug and optimized builds match 5,288 raw original cases, 36 original probes stopped before unresolved calls, and 960 additional real-model-vtable animation cases (832 direct dependency calls and 128 composed caller cases). Prefix writes and explicit dependency errors are preserved. The unlisted LS3DF dependency at RVA 0x33890 has no canonical entry credit; StopAnimation is an upstream PS2 research name. Full first MISE01 lock, actor/Joint construction, collision, camera, vehicle physics and controllable gameplay remain unavailable. |
| LS3DF owned Joint state and linked invalidation | [crates/lhr-engine/src/joint_state.rs](../crates/lhr-engine/src/joint_state.rs) | 3 | 1335 | Static inspection | Three bounded PC entries retain written constructor fields, an unavailable unwritten matrix cache, complete parsed 68-byte Joint records and linked singlemesh counter/flag/bound invalidation. Rust matches 1,335 admitted original cases and 350,365 projected words: four constructors, 1,296 updates, 32 mixed traversals and three complete loads. The broader 2,250-case native corpus includes 915 excluded sector or partial/error/failure-configured cases. Separately, 5,672 original Joint records preserve 385,696 raw bytes in 315 detached synthetic envelopes with remapped parents; this is not full character loading or original hierarchy parity. Singlemesh virtual updates explicitly fail; GetJointMatrix, binding/deformation, native allocation/I/O/destruction and gameplay remain unavailable. PS2 names are original, but PC masks, fields and the 68-byte payload are verified separately. No application Joint-model integration claim. |
| Game original animation index lookup | [crates/lhr-engine/src/player_control.rs](../crates/lhr-engine/src/player_control.rs) | 1 | 1636 | No integration recorded | Original GetAnimIndex semantics over a caller-owned sorted table: unsigned code comparisons, signed index results, null/missing returns and deterministic duplicate midpoint choice. All 1,636 admitted original comparisons match debug and optimized Rust. Nonnull empty tables and unrepresentable native record extents are rejected by explicit host admission; five native-only zero-count observations are excluded. Actual PS2 linker symbols and instructions support the name/cross-platform correspondence, without PS2 execution credit. No original table catalog is bundled; bootstrap, resource loading, PersonAnim, actor construction and first-mission execution remain open. |
| LS3DF Joint matrices and owned singlemesh binding | [crates/lhr-engine/src/frame_transform.rs](../crates/lhr-engine/src/frame_transform.rs) | 3 | 902 | Static inspection | Three bounded PC entry projections cover guarded Joint matrix evaluation and cache reuse, child/sibling joint discovery capped at 64, and owned binding-table/backreference stores. These scopes match 902 original cases and 551,594 represented u32 fields. Loaded definitions additionally compose binding with eager bounds callbacks and graph invalidation; 896 rich-bounds comparisons are separate from native DBuild execution. An independent 2,264-case inherited-update comparison supports mixed model/singlemesh/joint traversal without adding entries for already credited Frame Update or visual UpdateV. Missing pointer slots, unsupported kinds and numeric operations fail explicitly. Native allocation, driver resources, proxy dispatch and full object lifetime remain outside scope. Verified GetVertexJoint implementation is documented separately; its new canonical-entry credit awaits inventory admission. |
| LS3DF CPU skin deformation and normal-matrix arithmetic | [crates/lhr-engine/src/singlemesh_deformation.rs](../crates/lhr-engine/src/singlemesh_deformation.rs) | 4 | 4296 | Static inspection | Three arithmetic leaf projections and the CPU arithmetic snapshot of UpdateLodVertices match 4,296 admitted original cases and 205,472 u32 fields; four native zero-column normalization cases are explicitly unavailable. Rigid and unclamped blended position/normal operations retain original order and PC24 rounding, with exact suffix copies. The 1,000 LOD cases use supplied cached joint matrices and source/skin buffer pointers, including 500 controlled first allocations; they do not establish original driver, lock, proxy, lighting, cache or resource lifetime behavior. The two CPU helper names are descriptive because original names are unresolved. This SetRot matrix overload at RVA 0x2e1f0 is distinct from the already credited quaternion overload at 0x2e1c0. |
| LS3DF populated singlemesh bound-volume arithmetic | [crates/lhr-engine/src/singlemesh_bounds.rs](../crates/lhr-engine/src/singlemesh_bounds.rs) | 1 | 1024 | Static inspection | The populated-definition arithmetic projection of UpdateBVolumeProc preserves base-box inclusion, native corner and matrix-callback order, null-joint and invalid-X skips, strict min/max comparisons, and rounded center/radius calculation. All 1,024 original physical-x86/QEMU cases match Rust bounds; the same cohort separately matches graph callback matrices and flags. Consumed arithmetic requires finite normal f32 or zero under PC24/RN. Missing mesh/LOD/definition fallbacks, binding counters, visual flags and ancestor invalidation are outside this calculator and handled only by the separately bounded graph integration. No additional entry is claimed for the corner helper or UpdateBoundProc. |
| Game crouch posture and collision-filter state | [crates/lhr-engine/src/player_control.rs](../crates/lhr-engine/src/player_control.rs) | 3 | 2836 | No integration recorded | Three complete projected PC control/state bodies match 2,836 original cases in debug and optimized Rust: 1,836 crouch operations and 500 cases for each collision-owner setter. Tests preserve guards, exact query parameters, callback/filter ordering, stale-field retention, nonzero-hit handling and explicit host failure prefixes. Actual frame-update and sphere-collision callees are instrumented boundaries, so this does not establish original collision geometry, actor construction, animation ownership, playable controls or mission execution. PS2 symbols support names and correspondence; PC platform differences are retained. |
| Game animation-index initialization and cleanup | [crates/lhr-engine/src/animation_index.rs](../crates/lhr-engine/src/animation_index.rs) | 2 | 756 | No integration recorded | Caller-owned code/resource records preserve sentinel scanning, required allocation, original specialized equal-key sort order and cleanup pointer/count effects. All 756 cases and 353124 records match native output; 12 unavailable-call prefixes remain explicit. Original catalog, resource cache, allocator/release implementations and actor integration are not supplied. |
| LS3DF morph weights and first-LOD deformation | [crates/lhr-engine/src/singlemorph_state.rs](../crates/lhr-engine/src/singlemorph_state.rs) | 3 | 280 | Static inspection | Bounded signed-weight normalization, strict thresholds, source-order region overwrite, explicit vertex associations and retained dirty buffers match 256 weight cases and 24 original Tommy variants. Initial morph and inherited skin operations feed the sampled-pose renderer. Missing native index storage is unavailable; no native resource/buffer lifetime or complete LoadImage claim. |
| LS3DF bounded linear and spherical quaternion interpolation | [crates/lhr-engine/src/rotation_animation.rs](../crates/lhr-engine/src/rotation_animation.rs) | 1 | 22103 | Static inspection | The existing Slerp entry now admits checked linear and spherical branches: 22103 exact physical-native leaf cases from 24050 inputs; 1947 exceed the 1.5-radian spherical domain. PC24 arithmetic and outward transcendental intervals must produce one unambiguous float result; other numeric domains return explicit errors. Actual rotation tracks match 99579 evaluator samples. Existing Slerp, evaluator and normalization entries receive no duplicate credit. Multi-slot blending, scale and notes remain open. |
| LS3DF Target links and deferred direction updates | [crates/lhr-engine/src/target_state.rs](../crates/lhr-engine/src/target_state.rs) | 6 | 675 | Static inspection | Retains the prior 669 heterogeneous Target cases and adds six persistent lifecycle scenarios as a distinct cohort. Target teardown clears links without releasing members and swap-removes registration; first-owner removal is observed in one of those scenarios. New scopes exclude queued or duplicate-membership destruction. Standalone DeleteFromTarget597f0 remains uncredited. Earlier static inspection use is retained; no new rendered, interactive or whole-game teardown verification. |
| LS3DF parent-relative SetDirWorld and Target graph integration | [crates/lhr-engine/src/frame_transform.rs](../crates/lhr-engine/src/frame_transform.rs) | 1 | 70 | Static inspection | Parent-present SetDirWorld with near-zero roll matches 70 original cases over seven parent matrices; 14 additional cases return explicit unsupported arithmetic errors. Parent cache, inverse fallback, direction matrix, local basis stores and flags retain original order. Target queue flush occurs after root dirty clearing, preserves duplicates/reentrancy, and poisons interrupted graph updates. Parentless and nonzero-roll paths remain unavailable. |
| LS3DF Target direction-matrix arithmetic | [crates/lhr-engine/src/target_direction.rs](../crates/lhr-engine/src/target_direction.rs) | 1 | 4493 | Static inspection | The original direction-matrix leaf matches all 4493 physical-x86 and QEMU cases, including axis, signed-zero, tiny, threshold-neighbor and random vectors. Each consumed PC24 intermediate must remain normal or zero; unavailable exponent and nonfinite inputs return errors. It feeds the bounded SetDirWorld path without replacing original math with generic look-at behavior. |
| LS3DF owned IPolSet lifecycle and controller controls | [crates/lhr-engine/src/animation_controller.rs](../crates/lhr-engine/src/animation_controller.rs) | 13 | 27 | Static inspection | Thirteen canonical entries cover eight-slot ownership, exact-name binding, evaluator growth and reverse-prefix cleanup, Clear and existing-controller controls. Physical native O0/O2 and public Rust debug/release match 2908 control words across 27 snapshots. Allocation, release, error reporting and destination marking remain explicit backends. PC has eight slots, PS2 three. Raw inactive channel pointers, pose resources, callback reentrancy and unmasked hardware exceptions are outside scope. Masked signaling-NaN stores are quieted and checked in eight additional native cases totaling 48 words. Inline construction and destructor close with extra detached-allocation host reclamation receive no entry credit. |
| Game existing animation-machine track lookup | [crates/lhr-engine/src/actor_animation.rs](../crates/lhr-engine/src/actor_animation.rs) | 1 | 69226 | No integration recorded | Reverse existing-record lookup matches 3201 scripted required-comparison cases and 66025 cases with unchanged native C-locale CRT, including 65025 name-byte pairs; three unavailable-call prefixes are separately retained. Signed nonnegative truncated record counts and wrapping opaque identities preserve native read/call order. Actual original-locale comparison is a required backend; CRT and resource construction receive no credit. PS2 name correspondence uses stride96 and name offset48 versus PC stride100 and offset0, with differing guards. |
| LS3DF loaded singlemesh bounds-cache gate | [crates/lhr-engine/src/frame_transform.rs](../crates/lhr-engine/src/frame_transform.rs) | 1 | 206 | Static inspection | The exact 39-byte canonical entry compares loaded joint-change and cached counters, invokes existing bounds arithmetic when unequal and marks equal-counter bounds valid. Native physical/QEMU comparison covers 203 changed and three unchanged entries plus a second unchanged query per record; the composed production path matches 1413160 words over 206 persistent steps. Caller requests bounds and CPU deformation explicitly. Missing-resource fallbacks, native renderer scheduling and tail-transfer alias 0x57580 receive no extra credit. |
| LS3DF bounded model Close and destruction | [crates/lhr-engine/src/model_loading.rs](../crates/lhr-engine/src/model_loading.rs) | 2 | 7 | No integration recorded | Seven persistent original-call scenarios cover forward owner releases, surviving links, nested model teardown, retained Close capacity and retired identities. Native filenames and controllers are absent; consumed host playback is separately owned. Heap free order, real file Open, scene and loaded visual-resource destruction are excluded. New lifecycle APIs have no application-runtime integration receipt. |
| LS3DF bounded base, dummy and unbound Joint destruction | [crates/lhr-engine/src/frame_transform.rs](../crates/lhr-engine/src/frame_transform.rs) | 3 | 10 | No integration recorded | Ten persistent scenarios compare represented base detach and non-owning child links; nine include dummy teardown and four include unbound Joint teardown. Associated-singlemesh Joint cleanup has static and unit evidence only. Weather, fast-find, shadow, tick and unsafe queued or duplicate Target states are unavailable. Native lastDestroyedFrame bookkeeping, allocator identities/events, driver/visual-resource and application-runtime teardown are excluded. These are bounded field projections, not complete destructors. |
| LS3DF bounded visual-resource primitives | [crates/lhr-engine/src/visual_resources.rs](../crates/lhr-engine/src/visual_resources.rs) | 9 | 6 | No integration recorded | Six persistent original-call scenarios compare bounded plain materials, face-group chains and mesh-level ownership: 172 normalized snapshots and 1260 applicable original-resource state cells including normalized identity/kind/liveness, with 254 external fixture cells separate. Twenty original-triggered dependency effects match; eight fixture-only releases are separate. Positive signed references, nonreentrant required backends and unique chain/block ownership bound the domain. Scalar destruction uses delete flag 1; raw/managed buffer release and texture aliases are covered. Native texture/IB/VB managers, allocator and pointer-return parity, animated materials, mesh-object/LOD, loaded frame/model teardown and application integration are excluded. No new rendered or interactive validation. |
| LS3DF bounded mesh-object ownership and bounds | [crates/lhr-engine/src/visual_resources.rs](../crates/lhr-engine/src/visual_resources.rs) | 3 | 10 | No integration recorded | Ten persistent original-call lifetime scenarios cover a ten-slot mesh owner, shared and duplicate level references, null truncation, hidden retained references and same-pointer bounds refresh. Bounds uses reversed slots, strict box ties and original Z/Y/X square-sum order in an admitted numeric domain. Separate numeric evidence matches 83 admitted raw bounds cases; six wider inputs are host rejections, not native behavior matches. UpdateBoundVolume 0x31400 remains uncredited because it is absent from the frozen inventory. Eighteen controlled IB/VB boundary callbacks have null descriptors and establish no native manager implementation. Host numeric rejection is an admission policy; native allocator/pointer-return parity, backend implementations, loaded visual teardown and application integration are excluded. No new rendered or interactive validation. |
| LS3DF bounded IB/VB buffer-manager ownership | [crates/lhr-engine/src/buffer_managers.rs](../crates/lhr-engine/src/buffer_managers.rs) | 18 | 14 | No integration recorded | Fourteen complete original-call trajectories cover bounded IB/VB block allocation, equal-size ties, splitting/coalescing, used swap removal, stale descriptors, reclamation, retries, unlock and ordered owner invalidation. A separate five-phase prefix of case7 matches before the host rejects new insertion into an unsorted mutable-key manager tree; native continuation is excluded. New insertion requires nondecreasing retained inorder; existing reuse/free preserve actual order. Complete cases match 352 ordered required boundary effects; the prefix adds 20. Normalized object allocator/COM/driver/diagnostic callbacks are controlled dependencies, not backend implementations; extra host callback parameters and VC6 container allocations are excluded. Returning fatal-handler fixtures do not establish whole-game OOM recovery. Three composed constructors receive no credit. No visual-owner bridge, loaded teardown, renderer/application integration or new rendered/interactive validation. |
| LS3DF per-object plain LOD resources | [crates/lhr-engine/src/i3d_object.rs](../crates/lhr-engine/src/i3d_object.rs) | 4 | 8 | No integration recorded | Eight original-call trajectories compare represented plain Lod construction, direct nondeleting cleanup, admitted scalar flags1 cleanup/logical retirement and the object resource cleanup prefix. All ten unique generated LODs clean up ascending before the entire slot array clears and shared mesh Release begins, with raw mesh field unchanged. Nineteen immediate constructor snapshots compare 13 initialized data fields; machine vtable/unwritten padding are native-only checks. The scalar selfdelete/pointer ABI is not reproduced. The object base visual/frame call is controlled and excluded; native slot/base watchers establish original ordering without matched Rust callback-time alias snapshots. State comparison excludes cleanup-complete markers and freed payload. 19 primitive heap deletes remain original-only. 4 direct numeric dependency returns, 18 fixture-admission booleans and 51 synthetic statuses remain separate. General SetMesh/Create/upload, derived LODs, texture/GPU/allocator implementations, loaded/application teardown and new rendered/interactive validation remain excluded. |
| LS3DF C_str ownership | [crates/lhr-engine/src/c_str.rs](../crates/lhr-engine/src/c_str.rs) | 3 | 10 | No integration recorded | Represented C_str and representation ownership: direct exact-zero release, direct nondeleting string destruction, and const-char assignment including NULL, nonnull empty and a view into old bytes. New bytes are copied before old reference release. Production operations and actual entry-specific original-call cohorts are bound in the private wrappers. Positive reference budgets, extent/identity guards and poison after backend failure are host policies. Returning-null allocation is source/unit/static evidence only. Native allocator implementation, this-pointer returns, terminal stale-pointer use, arbitrary wrapped counts and representation constructor entry 0x6fe20 remain excluded. |
| LS3DF nonmanaged ITexture lifetime | [crates/lhr-engine/src/itexture.rs](../crates/lhr-engine/src/itexture.rs) | 5 | 8 | No integration recorded | Constructor-written lifetime fields and positive nonmanaged Release, direct nondeleting destructor, Close and Unlock. Constructor-unwritten fields stay unavailable until host intake. Close replaces primary then alpha names, removes the first cache match with swap-last within admitted unique membership, and releases graphics last; final destructor releases alpha before primary. Surface/graphics, pixel and string callbacks are explicit controlled boundaries. Low16 levels is a source-assisted projection; highword/padding and machine vtable/this-pointer parity are excluded. Managed flags/record, duplicate cache intake, callback reentry, malformed/unbounded state, native Create/Lock acquisition/upload, primitive heap-selfdelete and loaded/application/GPU integration remain excluded. Safe host intake, budgets and failure poison are not native creation or error-path parity. |
| LS3DF managed texture removal | [crates/lhr-engine/src/texture_manager.rs](../crates/lhr-engine/src/texture_manager.rs) | 1 | 7 | No integration recorded | Seven admitted original-call cases compare serialized managed removal: reverse first matching active record, conditional 256-slot queue clearing, filename then record deletion before swap-last publication, independent three-pool reverse owner clearing, high-flag reset and genuine AL success. Active record order and represented post-call state are compared; inactive backing slots/capacity and dead payload are excluded. Included opaque record/pool words and out/put counters are preservation checks, not recovered accessed members. Direct removal retains raw managed/graphics fields; existing Close subsequently clears them without texture graphics Release. Explicit critical/allocator callbacks, seeded intake and bounded coherent identities/references are required. Two native-only unsafe queue trajectories and effectful backend failure/poison policies are not native differential matches. Original-only instruction tracing does not prove Rust internal callback alias/timing or concurrency. Worker/request/creation/upload, pool shutdown, GPU/backend implementations and loaded/application integration remain excluded. |
| LS3DF managed texture record creation | [crates/lhr-engine/src/texture_manager.rs](../crates/lhr-engine/src/texture_manager.rs) | 1 | 5 | No integration recorded | Bounded prepared-owner AddTextureToTextureManager: actual 20-byte record fields, raw-byte presaved filename derivation, explicit record/filename allocation and copy, genuine filename-allocation rejection and cleanup, texture pointer publication and registry append/growth. Host preparation leaves initialization/cache/flags outside this helper. Existing removal registry storage writes are supporting extension evidence with zero new entry credit. Required storage callbacks represent selected original stores/copies separately from allocator/diagnostic calls; they do not establish arbitrary memory, pointer ABI or native heap implementation parity. Original publication stores are native trace evidence; Rust internal callback alias/publication timing equality is excluded. Stable format/list state, NUL-free names 5..253, bounded arithmetic, compatible identities, exclusion of ALL historical manager allocation tokens and nonreentrant backends bound the host domain. Unchecked record/list allocation rejection and backend-error poison are host policies. Full Create, Request/worker/upload, pool shutdown, GPU/backend and loaded/application integration remain excluded. |
| LS3DF managed texture request producer | [crates/lhr-engine/src/texture_manager.rs](../crates/lhr-engine/src/texture_manager.rs) | 1 | 8 | No integration recorded | Bounded managed texture queue producer at RVA 0x7a910; Request is an upstream working name without original PS2 name evidence. Nonmanaged availability, managed last-frame refresh before loaded test, already-loaded availability and weak queue enqueue with signed length comparison are represented. Eight admitted cases compare ten Request calls plus three existing Remove/Close calls: 13 return rows, 13 state rows and 41 ordered effects in debug/release, counted once. Actual Create precedes Request in three cases; other managed cases use explicit seeded Rust records. Selected original record-store instructions compare to Rust storage effects, not native callbacks. Queue intermediate stores remain native-only instruction evidence. Four unadmitted native calls, negative/257 intake and stale-record host guards, worker/upload, full Create, pool shutdown, concurrency, GPU and loaded/application integration are excluded. |
| LS3DF standalone managed texture pool unload | [crates/lhr-engine/src/texture_manager.rs](../crates/lhr-engine/src/texture_manager.rs) | 1 | 3 | No integration recorded | Bounded standalone managed texture pool unload at RVA 0x7afe0; UnloadManagedTextures is a pinned mafia-re working name without recovered original PC/PS2 symbol. One critical-section pair surrounds three pools in order and reverse slot visits; live owners have only the loaded bit cleared, then pool slot owners are nulled. Already-unloaded and queued owners remain admitted. Registry/record/queue state, pool storage/length/capacity/grow fields and graphics handles are preserved. No COM release, refcount, allocator, worker, image loading, device, restoration, whole SceneClose or application integration credit. Three admitted cases compare five unload calls plus one existing Request and one removal: seven state rows, seven return rows and 17 effects in debug/release, counted once. Rust seeds pre-Unload state; native creation/pre-Unload loaded Request are context only. Native-only 439-instruction/16-store trace and byte preservation do not establish internal Rust store observer parity or raw borrowed-graphics equivalence. Host managed-bit, record/queue coherence, live identities and bounded extents exceed native owner-pointer/nonzero-count tests; dedicated unload intake admits already-unloaded owners. |
| LS3DF standalone managed texture pool release | [crates/lhr-engine/src/texture_manager.rs](../crates/lhr-engine/src/texture_manager.rs) | 1 | 3 | No integration recorded | Bounded standalone managed texture pool release at RVA 0x7a610; ReleaseManagedTextures is a pinned mafia-re working name without recovered original PC/PS2 symbol. One critical-section pair clears loaded bits and weak owners across three pools with reverse slot visits. After unlock, each pool releases optional system surface then texture, releases each reverse-visited slot surface then texture, frees its array and zeros data/count/capacity and pool memory while preserving grow. Shared COM identities represent separately retained references. Registry/record/queue state and borrowed texture graphics are preserved. No Restore, worker, whole SceneClose, real Direct3D destruction, device, rendering or application integration credit. Three seeded scenarios compare six release calls: six state rows, six synthetic void return-class rows and 108 ordered effects (12 lock, 78 controlled COM Release, 18 allocator calls), counted once across debug/release. Controlled identities start at 100 references; actual Direct3D destruction is untested. Native fixture Create is context only, with no Rust Create comparison. Native-only 1,404 instructions/106 stores and byte preservation do not establish Rust intermediate store observer parity. Nonnegative bounded extents, live owners, valid slot COM identities and allocator ownership, sufficient alias lifetimes and nonreentrant/nonconcurrent backends bound admission. |
| LS3DF standalone managed texture pool restore | [crates/lhr-engine/src/itexture.rs](../crates/lhr-engine/src/itexture.rs) | 1 | 6 | No integration recorded | Bounded standalone managed texture restore through controlled device/storage effects. Calls accepted Release dependency, selects raw format once from flags and globals, then reconstructs three system/default pools in order. Uses returned raw descriptor Size, signed budget comparisons and wrapping memory arithmetic; sets grow1024, expands pool capacity in1024 steps, copies eleven-dword records and frees old arrays. Reserves registry to signed max(count,4096), retaining count and entries. Existing dependencies receive no duplicate entry credit. Six admitted cases compare 5,565 ordered effect rows and 1,109 state rows once across debug/release, spanning eight Restore and ten existing Release calls including six effect-only cleanup phases. No return/status or Rust instruction/store comparison. Native-only case six has fully backed count4097/capacity4096 but fails host coherent intake. Controlled device/storage effects, coherent live identities, representable extents, separately owned alias references and bounded terminating reconstruction constrain admission; no real GPU, full texture Create, worker or application integration. |
| rw_data read-mode Seek and loose-file Close | [crates/lhr-assets/src/rw_data.rs](../crates/lhr-assets/src/rw_data.rs) | 2 | 38 | No integration recorded | Read-mode Seek and common loose-file Close observable returns under unchanged original DLL exports in Wine/QEMU: 38 observations, comprising 32 Seek and six Close results, counted once across debug/release. Four Open alignment rows and PID metadata are excluded. Sizes 0, 1, 10 and 32770 cover failed-seek position preservation, EOF+1, invalid/closed handles and slot-zero reuse. No native internal cache/block inspection, write fallback, buffered delete/rename, archive ownership, free-order, locking, Read/Open or whole-DLL parity. Full modeled branches retain separate static-only checks. Snapshot intake and serialized worker composition are host adapters, with no application/GPU integration. Fixture exits zero; two helper startup/shutdown QEMU SIGSEGV messages have unknown exact helper identity and do not establish overall Wine stability. |
| rw_data loose-file dtaRead | [crates/lhr-assets/src/rw_data.rs](../crates/lhr-assets/src/rw_data.rs) | 1 | 27 | No integration recorded | Bounded loose-file dtaRead only: 27 unchanged-original-DLL READ observations under Wine10/QEMU match debug/release once. Eleven SEEK context rows and four OPEN alignment rows add no credit. Buffer comparison uses 32-bit FNV hashes, not full byte equality. Each fixture Read is followed by Seek(0,CUR), which normalizes block cursor; replay follows that exact stateful trajectory. Separately, 2,948 static-model cases compare admitted returns, numeric/cache-validity/physical state and effects with buffer hashes. Runtime internal cache fields and partially overwritten still-valid cache reuse remain unproven. Five private host-error review scenarios are separate from native and static-model case counts. Coherent bounded loose-handle/cache intake is a host adapter. Archive decoding, native Open/Create, allocation/locking/concurrency and whole dtaRead/DLL parity remain open. Fixture exits zero; a separate helper shutdown fault leaves overall Wine stability unproven. No native hardware, game/GPU, application, rendered or interactive validation; worker original-runtime comparisons remain zero. |
| rw_data bounded native archive registration and Open | [crates/lhr-assets/src/rw_data.rs](../crates/lhr-assets/src/rw_data.rs) | 8 | 24 | No integration recorded | Bounded native archive registration, unlock, reference lifetime and Open compose with existing Read/Seek/Close. Eight canonical entries add one contract. Stored synthetic fixtures compare 81 native log rows and 976 complete-array bytes in debug/release, counted once: 24 opens, five creates, three unlocks, one AddRef and four Release calls; constructor/destructor occur within create/final Release. Separately, 21 scripted effect cases per profile establish Rust ordering, not native injected failures. ASCII names shorter than 260 bytes; serialized concurrency; timestamp metadata unmodeled; native leak behavior represented by abstract effects. Host short-index reads stop explicitly rather than modeling native uninitialized bytes. WAV and full DLL, application, GPU and mission execution remain open; no new rendered or interactive validation. |
| rw_data bounded PCM16 WAV archive decoding | [crates/lhr-assets/src/rw_data.rs](../crates/lhr-assets/src/rw_data.rs) | 1 | 202 | No integration recorded | Bounded PCM16 mono/stereo WAV decoding and archive header ownership compose with existing Open/Read/Seek/Close. One canonical PC entry adds one scoped contract; Open/Read/header extensions add zero entries. Native evidence: 202 Read observations (172 nonfailure, 30 released-handle failures), 66 explicit Seek context rows, 510 normalized log rows and 7,280,960 complete-buffer bytes including guards/unrequested regions across 26 archive fixtures plus four supplemental trajectories reusing four fixtures; debug/release counted once. These are archive API observations, not direct codec calls. Separately, 128 logical vectors include 56 exhaustive vectors (10,752 delta operations, 84 seeds) and 72 others; 18 scripted effects plus four direct-buffer effects do not claim native failure injection. Eight fresh REA results support the codec; exact PC/PS2 constants and original PS2 names add no PS2 runtime validation. Identifier-only header probe, header-only format bypass, mono/stereo block-align/channel quotient2 and all type codes at least8 default-row rules are admitted. Generic non16 LZ, other channels and malformed native memory behavior excluded. No application/audio playback, rendered, GPU or mission integration; Write and full DLL parity remain open. |
| Game bounded replay state and live VM wait | [crates/lhr-engine/src/replay_state.rs](../crates/lhr-engine/src/replay_state.rs) | 2 | 1161 | No integration recorded | Two canonical Game entries: busy leaf 0x19e2d0 and full state setter 0x1989a0 in admitted live storage, with descriptive working names. Actual original x86 instructions under QEMU match 345 leaf, 708 setter, 102 bounded-dispatcher and six scheduler cases; 2,972,160 normalized output bytes per profile include unused zeroed capacity, with debug/release counted once. Live object/vector/vtable replacement, callback order and reentry remain observable. RECWAITFOREND opcode 0x2e uses live busy state and existing common advance; this bounded dispatcher extension adds no entry or whole-dispatcher credit. Synthetic scheduler wrapper mutates its fixture global after dispatch, without running original replay Update. Separately 76 synthetic Rust cases cover host admission/errors and the authored MISE01 line-2 dim_act frontier. No original PC/PS2 replay helper names, owner construction, resource-backed Load/Update, camera/actor/mission, application, rendered or GPU integration. |
| LS3DF bounded frame lookup and Game lookup callers | [crates/lhr-engine/src/frame_lookup.rs](../crates/lhr-engine/src/frame_lookup.rs) | 2 | 207 | No integration recorded | Two bounded canonical LS3DF entries: scene FindFrame 0x49210 and frame GetChildFrame 0x1c360, flags exactly 0x4ffff, ASCII names and separately admitted default-C Game/LS3DF locales. Game FINDACTOR/FINDFRAME dispatcher arms add zero canonical entries. Actor reverse primary, forward globals then reverse secondary preserves miss and partial-write behavior; frame self/child/sibling primary-before-backdrop lookup clears a miss. Original x86 under QEMU matches 207 unique inputs: 189 Game dispatches and 18 direct LS calls, 13,248 normalized bytes per profile including 168 zero-padding bytes, counted once across debug/release. Sole MessageBeep IAT replacement produces six actual callbacks; 35 secondary checkpoints remain native-only. Separate 108 Rust probes comprise 107 synthetic cases and one actual authored-script intake. Synthetic ModelArena load/link/close/reopen and stale identities do not establish mission-world integration. Host errors, lifetimes and budgets are admission policies. No original world/source registration/removal, allocator, Windows audio, application, replay resource, rendered or GPU integration; whole-function coverage N/A. |
| Game bounded base actor lifetime and source ownership | [crates/lhr-engine/src/actor_world.rs](../crates/lhr-engine/src/actor_world.rs) | 11 | 88 | No integration recorded | Eleven bounded Game entries cover owned base actor fields/lifetime, real kind-1 factory, separate static/primary/active membership, manager flag-zero cleanup and deleting wrapper. Release0x91d0 is implemented but absent frozen inventory: zero entry credit. 88 of 93 original-instruction inputs match 85536 normalized bytes per profile, counted once across debug/release; two repeated-construction and three flag-one cases stay native-only. Caller growth/copy/commit executes; allocator/free and frame virtual implementations remain controlled required effects, including targetE/targetN flags0x40. AX-only Release result and wrapper effects are compared; wrapper original-this EAX is excluded. 140 independent Rust probes comprise 138 synthetic and two source-intake cases, with zero original executions. Host lifetime/budget/error rules are admission policies. No mission-world construction, derived actor/payload, application/render/GPU integration; whole-function coverage N/A. |
| LS3DF bounded actor frame effects over owned ModelArena | [crates/lhr-engine/src/actor_frames.rs](../crates/lhr-engine/src/actor_frames.rs) | 3 | 151 | No integration recorded | Three bounded LS3DF entries: SetOn 0x1b400, FindChildFrame 0x1c330 and fixed-search EnumSubtree 0x1c020. Real ModelArena descendant search uses stored original-name+104 and Target class 7 with flags 0x40, nonempty ASCII/default-C queries; PC low-byte bit 0 SetOn; shared live external userdata records and supported positive-count owner release. Release 0x1aae0, EnumFrames 0x1c2a0 and callback 0x1c310 outside frozen inventory receive zero entry credit; script helper 0x1c360 unchanged. 151 of 474 native inputs match 75,500 bytes per profile; 323 stay native-only. General callback/global/status and lastDestroyedFrame reset, raw Release returns/pointers and deallocator internals excluded. Separate 166 Rust cases: 160 synthetic, two source and four resource attempts; three models / 51 frames, actual targetN only, all three model closes unsupported. Host lifetimes/limits/preflight errors are admission policies. No mission-world/application/render/GPU integration; whole-function coverage N/A. |
| Game owned program source and source-only detector lifetime | [crates/lhr-engine/src/original_program.rs](../crates/lhr-engine/src/original_program.rs) | 9 | 382 | No integration recorded | Nine Game entries cover raw/null/empty owned C_program source/name lifetime, default-C EqualFinalPath and reverse-first replacement, plus real source-only kind-5 detector constructor/deleting wrapper/destructor. Detector Init/LoadData are outside frozen inventory and factory/base/frame entries have prior credit: zero duplicates. 382 of 389 unique native inputs match 168585 bytes/profile; seven native-only and duplicate 184 excluded from 390 IDs. Allocation/free/source I/O controlled; nonempty vector destruction, ignored/skipped reads and raw pointer/ABI returns excluded. Separate 300 Rust configurations: 263 synthetic, 33 source, four retained resource; 31 payload configurations cover 32 records, including duplicate 27/29. Uncertain free poisons owner without retry; other host errors preserve admitted prefixes. Source-only vectors/runtime buffers remain empty; no Compile/Init/Done, scheduling, mission-world/application/render/GPU claim. Whole-function coverage N/A. |
| Game original parser helpers and full instruction recognition | [crates/lhr-engine/src/program_parser.rs](../crates/lhr-engine/src/program_parser.rs) | 4 | 4727 | No integration recorded | Four scoped Game entries reconstruct byte tokenizer, quoted string and wrapping integer operand helpers plus all 398 instruction keywords in original order and IDs. Native comparison passes 4727 unique configurations across 4742 admitted rows, counted once across profiles; two empty-input rows excluded. Includes 400 unique original MISE01 line recognition inputs and 16 separate host-admission checks. ASCII/C-locale numeric and recognition paths, explicit NUL and signed-length bounds; original mutation and output sentinels retained. No float/LET/IF, parameter decoder, Compile, scheduling or mission integration; whole-function coverage N/A. |
| Game bounded original vehicle control setters | [crates/lhr-engine/src/vehicle_controls.rs](../crates/lhr-engine/src/vehicle_controls.rs) | 5 | 2373 | No integration recorded | Five scoped Game entries preserve SetPower, SetBrake, SetClutch, SetSteer and SetGear routing, shaping, returns and ordered writes in caller-supplied vehicle storage. 2373 of 2851 unique native configurations match full state and guards in debug/release; 478 numeric-domain cases remain native-only. Ten separate first-access breakpoint probes verify short-storage prefixes without claiming native memory-fault behavior. Explicit CW007f comparison and host nearest-rounding precondition; raw copies preserve bits, required arithmetic is bounded. PC rejection and PS2 clamping differ. No vehicle constructor, parameters, BODY, wheels, dynamics, application controls or driving integration; whole-function coverage N/A. |
| Game bounded collision triangle leaf query | [crates/lhr-engine/src/collision_line.rs](../crates/lhr-engine/src/collision_line.rs) | 1 | 1287 | No integration recorded | One partially reconstructed canonical Game entry covers all seven triangle projection branches of LineInTreeLevel, ordered candidates, masks, one-sided plane test, strict nearest-hit comparison and raw stored normal. 1287 CW007f configurations match complete native fixture bytes in debug/release; one unsorted fixture is host-excluded. Other precision profiles produce 377 differences each and receive no parity credit. Twelve independent host/property tests supplement source tests. Caller supplies sorted bound triangles, bounds and initial best fraction; zero or magnitude 2^-40 through 2^40 arithmetic and host nearest rounding are explicit. Unsupported volume/numeric errors are transactional host policy. No KLZ loader/grid, TestLineV wrapper, dynamic collision, wheel contacts, solver or driving integration; whole-function coverage N/A. |
| Game bounded original vehicle constructor | [crates/lhr-engine/src/vehicle_owner.rs](../crates/lhr-engine/src/vehicle_owner.rs) | 1 | 1024 | No integration recorded | One canonical C_Vehicle constructor entry preserves ordered stores, original constants, the native-this high-byte member markers and untouched caller storage. 1024 native configurations match complete fixtures and defined EAX in debug/release; 32 separate constructor-to-five-controls trajectories match 192 stage observations. Twenty-one native pre-access prefix probes reuse an existing full configuration and add no full-case credit; Rust Storage failures are host policy, not native memory faults. Vec ownership/drop does not run original resource lifetime. Actual PS2 names/layout differences are recorded. No Init, destructor, BODY/model/wheel resources, solver or gameplay integration; whole-function coverage N/A. |
| Game bounded original vehicle drivetrain arithmetic | [crates/lhr-engine/src/vehicle_drivetrain.rs](../crates/lhr-engine/src/vehicle_drivetrain.rs) | 4 | 5215 | No integration recorded | Four bounded C_Vehicle entries reconstruct SetCutTime, ComputeMoment, MotorForce and MotorRot over supplied storage and an explicit shared direction byte. 5215 of 5267 unique native configurations match full state, defined return and direction in debug/release; 52 numeric-domain cases are excluded. Explicit CW007f/PC24 nearest comparison, caller-established host nearest rounding and finite-normal-or-zero arithmetic apply; raw-copy paths retain payload bits. Original knot boundaries, constants and write order are preserved. PC multiplies milliseconds by binary32 0.001; PS2 division/layout/signature differences are not imported. Unavailable shift callback and short-storage prefixes have instruction/Rust evidence only, not native callback completion or faults. No parameter loading, Engine/Move, wheel/body solver or gameplay integration; whole-function coverage N/A. |
| Game bounded vehicle Init attachment and discovery | [crates/lhr-engine/src/vehicle_init.rs](../crates/lhr-engine/src/vehicle_init.rs) | 1 | 239 | No integration recorded | One canonical C_Vehicle::Init entry contributes only its attachment, name allocation and BODY/component discovery prefix. 239 unique unchanged-original-instruction fixtures match debug/release state, defined rejection AL and ordered effects under QEMU with modeled enumeration/allocation/free; actual callback bodies execute. 102 original rejections and 137 unfinished EXHAUST/SPECCOLL/HEAD/BODYCOL frontiers are separate outcomes. Retained references and temporary vectors remain explicit. No outcome reports completed initialization; component transforms, parameters, wheels, cleanup/destruction and gameplay remain unavailable. Callback entry boundaries add zero credit. Whole-function coverage N/A. |
| Game bounded collision triangle preparation | [crates/lhr-engine/src/collision_prepare.rs](../crates/lhr-engine/src/collision_prepare.rs) | 1 | 293 | No integration recorded | One canonical classifier entry supplies bounded triangle classification and ordered cell application; associated frame degeneracy comes from a partial LoadCollision projection with zero extra entry credit. 293 of 322 unique fixtures match in both Rust profiles; 26 numeric-domain exclusions and three explicit binding diagnostics remain separate. The unchanged classifier executes on physical x87 for all 322 fixtures; 196 original loader fixtures use QEMU with modeled allocation/archive/frame/LOD0 effects. Physical classification consumes QEMU loader-derived disabled flags where applicable, not a wholly physical loader run. Seven QEMU threshold discrepancies are adjudicated by physical original instructions. CW007f/PC24 nearest and bounded used arithmetic apply. Original vertex/candidate order, planes and other property bytes are preserved; world-pool degeneracy bypass remains distinct. No scene ownership, complete loader, all primitives, world queries or gameplay integration; whole-function coverage N/A. |
| Game bounded original static grid segment query | [crates/lhr-engine/src/collision_query.rs](../crates/lhr-engine/src/collision_query.rs) | 1 | 2845 | No integration recorded | One canonical g_collision::_TestLineV entry reconstructs bounded general-segment static grid traversal, forced mask 0x20, strict nearest/first-tie behavior, optional point and original normal conversion, and last-cell state. 2869 unique original fixtures run unchanged on both physical x87 and QEMU with identical outputs; 2845 are admitted and match actual Rust debug/release, with 24 explicit host exclusions. Fifty-four duplicate fixture aliases add no credit. Return primitive, point/normal and state/guards match; the Rust-only fraction is not independently compared. CW007f/PC24 nearest, zero-or-2^-40..2^40 used arithmetic, signed32 grid conversion and prior leaf ordering prerequisites apply. Eligible volumes and dynamic dispatch are unsupported errors, never silent misses. No complete collision world, contacts, original physics solver or gameplay integration; whole-function coverage N/A. |
| Game static volumes and LS3DF local transforms | [crates/lhr-engine/src/collision_volumes.rs](../crates/lhr-engine/src/collision_volumes.rs) | 4 | 7304 | No integration recorded | Four retained canonical entries reconstruct the static volume worker, entry-face AABB helper and distinct LS3DF mutating point transform/RotateByMatrix. Five original classes preserve raw normals, near-root or entry-face rules, strict nearest ties and original candidate order. 7304 of 7371 unique direct original-instruction inputs match both Rust profiles; 67 explicit host exclusions remain separate. Another 2129 mixed triangle/volume static-grid inputs match original record, optional point/normal, cell state and guards without exclusions; their Rust-only query fraction is not independently compared. Direct worker fraction is compared; existing TestLineV/triangle entries receive no new credit. All native direct/mixed outputs agree between physical x87 and QEMU; original Game/LS3DF text remains unchanged and real transform exports execute. CW007f PC24 nearest and zero-or-2^-40..2^40 used arithmetic bound admission; wrapper-generated end/length fields and no callback/alias observation limit the safe API. Dynamic registration/query, complete loader/world transforms, contacts, response, solver and gameplay remain unavailable. Whole-function coverage N/A. |
| Game bounded GearSnd and MotorRot callback composition | [crates/lhr-engine/src/vehicle_sound.rs](../crates/lhr-engine/src/vehicle_sound.rs) | 1 | 4031 | No integration recorded | One canonical C_Vehicle::GearSnd entry reconstructs enable/query gates, shared original CRT random progression and bank choice, ordered SetOn/SetVolume effects and the actual MotorRot shift callback before rereading pending gear. Actual PS2 linker evidence corroborates the name; PC behavior follows unchanged original instructions. 4031 unique original PC inputs match Rust debug/release without exclusions: 1584 direct sound inputs and 2447 composed MotorRot inputs, including 1699 producing sound calls. All 5267 previous drivetrain inputs are physically reexecuted with zero QEMU differences; 5215 prior admitted rows still match, with the same 52 numeric exclusions. Explicit controlled sound methods expose callback-time state and mutations; the original audio backend and playback remain unavailable. Existing MotorRot and inline CRT rand add zero entry credit. No full Init, driving, rendered, interactive or mission validation; whole-function coverage N/A. |
| Game bounded BODYCOL record lifecycle | [crates/lhr-engine/src/vehicle_bodycol_records.rs](../crates/lhr-engine/src/vehicle_bodycol_records.rs) | 11 | 478 | No integration recorded | Eleven retained canonical record helpers reconstruct bounded deep copy, assignment, insert/growth, backward grouping, nested destruction, scalar deletion, size and placement copy/fill. Actual original instructions compare 478 unique physical inputs against Rust debug/release: 477 normal completions and one actual invalid-memory fault whose completed byte/effect prefix matches a Rust backend error, without claiming successful native completion. All 477 normal inputs agree with QEMU. Every 128 KiB region byte including seeded padding, allocation/free order and defined return projection compare; frames/reference sentinels remain unchanged. Allocation/free are explicit controlled effects, without original CRT heap, address reuse, C++ exception or reentrant allocator parity. Other dossier bodies, eliminated RET8, inline constructor/CRT/raw-copy projections and repeated callers add no new credit. No full vehicle Init, frame ownership, gameplay or driving validation; whole-function coverage N/A. |
| Game bounded BODYCOL callback and recursive passes | [crates/lhr-engine/src/vehicle_bodycol.rs](../crates/lhr-engine/src/vehicle_bodycol.rs) | 3 | 164 | No integration recorded | Three retained canonical entries reconstruct BODYCOL collection, reverse recursive descendant enumeration and transform/detach passes, composing actual record lifecycle. The actual descendant callback at RVA0xd1520 is implemented and compared but absent from the frozen inventory, so adds zero entry credit. 164 unique original-instruction cases match Rust debug/release, counted once: 81 isolated cases and 83 normal-entry Init compositions (64 bounded normal continuations and 19 effect-failure prefixes), physically executed with controlled external effects; no new QEMU execution is claimed. Required allocator, enumeration, frame update/link/remove and other external object effects remain controlled boundaries; original reference ownership and backend implementations are not established. All 29 direct callback inputs additionally compare the full 116-byte live original stack record. Other nested/Init caller scratch, three undefined special-control padding bytes per native record and diagnostic event metadata remain outside the admitted projection. Opt-in Init composes actual BODYCOL behavior without the previous three BODYCOL stubs, retaining completed effects and single-use continuation. No original Init success is reported; DOOR/BALLAST, parameters, wheels and genuine destruction remain unavailable. Existing Init, mutating LS3DF transform and record dependencies add no duplicate credit; whole-function coverage N/A. |
| LS3DF bounded fast frame tree and Game link continuation | [crates/lhr-engine/src/frame_fast_lookup.rs](../crates/lhr-engine/src/frame_fast_lookup.rs) | 4 | 31 | No integration recorded | Four retained canonical LS3DF entries reconstruct exact FindFrameFast_ tree traversal, RemoveFromFFTree, AddToFFTree and SetName with observable allocation effects. Actual PS2 symbols establish names; independent PC instructions establish tree/scene/layout behavior. 31 unique physical original-instruction cases compare 1436 ordered operations and 117729024 arena/guard bytes per Rust profile, counted once; no QEMU execution was performed. SetName includes 21 operations: 19 returns and two allocation-failure prefixes stopped at the actual fatal-handler entry, without native successful-return or original heap/handler parity. Seeded FindFrameFast0x19b10 lacks frozen membership and contributes zero new entry credit; existing SetOn and bounded LoadCollision link-loop continuation likewise add zero. Caller-supplied frame/scene identities establish storage only, without original constructors, driver ownership, LinkTo or live loaded worlds. Actual-map missing frames remain unchanged. No whole-map original query, complete loader, gameplay, rendered, interactive or driving validation; whole-function coverage N/A. |
| Game bounded component and EXHAUST record helpers | [crates/lhr-engine/src/vehicle_component_records.rs](../crates/lhr-engine/src/vehicle_component_records.rs) | 10 | 1153 | No integration recorded | New physical-x86 original-instruction cohort: 1,159 unique ABI/arena inputs, counted once across profiles; 1,153 normal completions and six separate native memory-fault prefixes. Ten canonical entries are rehashed against the actual Game.exe PE, durable fresh primary REA bodies and retained membership draft. Branch grids cover growth/in-place insertion, aliased sources including header words, forward overlap, full record/padding copies, null begin/allocation/placement, allocation/free-side header mutation, zero counts, and signed/wrapping size arithmetic. Full arena bytes, ordered allocation/free effects and defined returns or fault prefixes agree with current-source Rust debug and optimized builds. Allocator/free are controlled effects, not native CRT heap/lifetime ownership. This standalone record cohort establishes no full Init, actual scene ownership, driving, rendered or interactive validation. |
| LS3DF bounded shared scene AddFrame and Close | [crates/lhr-engine/src/scene_ownership.rs](../crates/lhr-engine/src/scene_ownership.rs) | 2 | 321 | No integration recorded | Two retained canonical LS3DF entries reconstruct bounded non-emitter scene AddFrame and Close in the same ModelArena/frame graph as models, names and parent links. AddFrame preserves reference-before-old-owner deletion, destination-owner-before-growth and visible allocation-failure prefixes; Close preserves owner-null-before-release, native forward list order, retained capacity/sectors and 13 explicit ordered resource boundaries. Native comparison covers 372 unique ABI/arena inputs, counted once across debug/release. The tabulated case count is 321 normal completions; 51 controlled host-effect prefixes remain separate. Compared live fields include references/owners, kinds/flags, hierarchy/transforms/names, membership order/capacity, sector links/bounds and per-Close-phase owner/reference/list snapshots. Full raw native arena, pointer/vtable/padding equality, original allocator/resource backends, emitter/tick registration, corrupted headers/reference wrap, reentrant callbacks and unavailable derived destruction are outside the admitted domain. Constructor stops before 0x48e29 and gains no credit; nonmember DeleteFrame and previously credited model/name/tree operations add no duplicate credit. No full driver/scene-resource construction, File Open/reload, vehicle BODYCOL adapter, gameplay, rendered or interactive validation; whole-function coverage N/A. |
| Game bounded explicit Vehicle destructor | [crates/lhr-engine/src/vehicle_destructor.rs](../crates/lhr-engine/src/vehicle_destructor.rs) | 1 | 7 | No integration recorded | Complete explicit C_Vehicle destructor in the admitted constructed-state domain, including rejected-Init persistent names/wheel nodes, ordered reverse releases, null/header stores and vector cleanup. Native free, virtual LinkTo/Release remain controlled required effects; no scalar-deleting wrapper, native resource destructor or whole car success. |
| Game bounded Vehicle UpdateAxis | [crates/lhr-engine/src/vehicle_axis_update.rs](../crates/lhr-engine/src/vehicle_axis_update.rs) | 1 | 84 | No integration recorded | Complete bounded reverse axis passes, enable transition and finite numeric geometry over genuine wheel/axis records. All 64 KiB and ordered effects exact; virtual SetDir input logging and SetOn mutation are controlled. Parent scratch is a capability fixture; helper-local stack bytes are not compared. Current production init source is an exact prefix of the oracle-derived file with only fixture seeding appended. |
| Game bounded source parameters, flags 0/1 | [crates/lhr-engine/src/vehicle_source_parameters.rs](../crates/lhr-engine/src/vehicle_source_parameters.rs) | 1 | 3710 | No integration recorded | Complete bounded InitCar source-parameter paths only for flags 0/1 and admitted finite numeric inputs, including source repair, table/bank selection, original true/false returns and live source/vehicle writes. 3,710 exact admitted inputs (3,445 true / 265 false) from 3,720 unique; 8 numeric and 2 native-fault inputs excluded. No flags 2/InitSounds or whole-helper parity; original whole-function coverage stays N/A. Actual Setup passes flags 0 and skips sound initialization. |
| Game bounded AXIS24 record insertion | [crates/lhr-engine/src/vehicle_axis_records.rs](../crates/lhr-engine/src/vehicle_axis_records.rs) | 1 | 242 | No integration recorded | Complete admitted 24-byte AXIS repeated insertion with signed/wrapping arithmetic, growth/in-place branches, live source/header aliases and ordered allocation/free effects. Full 64 KiB, effects and defined return projection match. One native fault is prefix evidence only; allocator internals are controlled. |
| Game bounded Setup12 record insertion | [crates/lhr-engine/src/vehicle_setup_records.rs](../crates/lhr-engine/src/vehicle_setup_records.rs) | 1 | 242 | No integration recorded | Complete admitted repeated 12-byte Setup insertion, growth/in-place branches, overlap/header aliases and completed allocation/free/store prefixes. Full 64 KiB, effects and defined return projection match. One native fault is excluded from completion; allocator internals remain controlled. |
| Game bounded special point and control insertion | [crates/lhr-engine/src/vehicle_special_records.rs](../crates/lhr-engine/src/vehicle_special_records.rs) | 2 | 164 | No integration recorded | Complete admitted 12-byte point insertion including original inserted-address return, growth/in-place storage, aliasing and allocator/free order. 82 completed first-instruction inputs compare all 64 KiB and effects; no allocator implementation credit. Complete admitted special-control insertion including original inserted-address return, growth/in-place storage, live aliasing and allocator/free order. 82 completed first-instruction inputs compare all 64 KiB and effects; no allocator implementation credit. |
| Game bounded C_car Init | [crates/lhr-engine/src/car_init.rs](../crates/lhr-engine/src/car_init.rs) | 1 | 32 | No integration recorded | Actual canonical C_car Init from native-constructor-produced explicit state; the original 13 nonlinear setter calls, AI00/AI01 markers, 160-byte records, seat/model metadata and admitted animation placement. Comparison covers 64 KiB persistent state, 703,960 bytes of Game.data, 624 active caller-frame bytes and ordered effects. Rust whole C_car construction and enclosing Game Init remain intake. |
| Game bounded explicit C_car destructor | [crates/lhr-engine/src/car_caller_cleanup.rs](../crates/lhr-engine/src/car_caller_cleanup.rs) | 1 | 3 | No integration recorded | Explicit C_car destructor closure in three admitted initialized/failed contexts with empty global pair lists. Only the actual false-flag branch of 0x59d1f0 is covered. Pair-removal iteration/erase/shift behavior has no native nonempty-list coverage and gains no new credit. Existing actor/Vehicle destructor dependencies keep prior credit; Game-owner resets remain separate. |
| Game bounded 160-byte car component records | [crates/lhr-engine/src/car_effect_records.rs](../crates/lhr-engine/src/car_effect_records.rs) | 1 | 9 | No integration recorded | 160-byte record insertion, overlap, growth/in-place storage and primitive allocation/free order. Compares all admitted persistent state and effects; no allocator implementation credit. |
| Game Init-reached car seat properties | [crates/lhr-engine/src/car_effects.rs](../crates/lhr-engine/src/car_effects.rs) | 1 | 13 | No integration recorded | GetSeatProperty only for actual C_car Init-reached seat-record queries in the 13 native main-corpus inputs. Covers admitted seat-record/property reads and local output state. Other query/transform modes and general world-space transform parity are unverified and excluded. |
| Game pure car animation ID selector | [crates/lhr-engine/src/car_effects.rs](../crates/lhr-engine/src/car_effects.rs) | 1 | 32 | No integration recorded | Pure integer C_human::GetAnimIDForUseCar ID selection for full byte ABI; does not create/load animation resources. |
| Game Init-reached car TCK placement wrapper | [crates/lhr-engine/src/car_effects.rs](../crates/lhr-engine/src/car_effects.rs) | 1 | 2 | No integration recorded | C_car::GetPlacementFromTCK only for two actual Init-reached high-bit/local-index native inputs. Covers the admitted wrapper path and its state/effect projection. Other index/query/transform modes, general world-space transform parity and native TCK IO/cache-miss heap ABI are excluded. |
| Game cached-TCK animation metadata placement | [crates/lhr-engine/src/car_frame_effects.rs](../crates/lhr-engine/src/car_frame_effects.rs) | 1 | 20 | No integration recorded | Game animation metadata placement in 20 actual direct cached-TCK inputs, comparing all three position and three direction float bits. Native parity covers admitted cached-track placement; the Rust adapter separately loads version-4 files into Game-owned typed metadata. Native file IO, cache-miss allocation, the original 100-byte cache heap and parser/allocator ABI are excluded. |
| Game bounded C_car constructor | [crates/lhr-engine/src/car_construction.rs](../crates/lhr-engine/src/car_construction.rs) | 1 | 34 | Static inspection | Canonical C_car constructor at 0x41bb80 over raw allocator-supplied storage. Thirty-four unique native this-return completions compare all 64 KiB persistent memory, 703,960 bytes of Game.data, the active 36-byte constructor frame, ordered allocation/free effects, ordered Game.data stores and live boundary snapshots. The original factory allocates 0x221c bytes; minimum represented object-memory span is 0x2214. Untouched bytes, cross-prefix aliases, caller scratch markers, base Actor/Vehicle composition and RNG/registry state are retained within admitted contexts. Two native NULL-memory-fault prefixes and two controlled host prefixes remain separate. The frame includes the SEH saved record; native C++ exception/unwind and Win32 SEH interoperability, allocator/free internals, all internal Car-store timing and all TLS/FS observer stores are excluded. Observed empty NULL-allocation survival is bounded evidence, not general nullable-new compatibility. Host failure checkpoints and no replay are Rust policies. |
| Game bounded Car and embedded Vehicle GameInit | [crates/lhr-engine/src/car_game_init.rs](../crates/lhr-engine/src/car_game_init.rs) | 2 | 40 | No integration recorded | Forty unique Car GameInit prestates execute actual Car and embedded Vehicle bodies over one live address map. Actor initialization and resource, shadow, sector and manager dependencies remain controlled effects. Complete persistent arena and Game.data plus ordered effects compare; private nested stack and unavailable external backends are excluded. This is not completed application GameInit or driving. |
| Game bounded Actor GameInit | [crates/lhr-engine/src/car_game_init_actor.rs](../crates/lhr-engine/src/car_game_init_actor.rs) | 1 | 8 | No integration recorded | Eight unique direct Actor GameInit prestates compare persistent arena and Game.data plus ordered frame and manager effects. The Actor call is controlled in the separate Car cohort; no duplicate input or full mission ownership is inferred. |
| Game bounded Car and Actor GameDone | [crates/lhr-engine/src/car_game_done.rs](../crates/lhr-engine/src/car_game_done.rs) | 2 | 18 | No integration recorded | Twelve direct Car and six direct Actor GameDone prestates compare complete 64 KiB arena, Game.data and ordered typed effects. Actual Actor cleanup also executes within Car cleanup. Vehicle GameDone, SoundOff, object removal, shadow, radar and traffic boundaries are controlled; no constructor-to-GameInit-to-GameDone lifecycle is established. |
| Game bounded vehicle sound deformation seats and doors | [crates/lhr-engine/src/car_game_init_vehicle_resources.rs](../crates/lhr-engine/src/car_game_init_vehicle_resources.rs) | 9 | 44 | No integration recorded | Forty-four unique core direct-entry inputs cover nine canonical seat-mass, sound, deformation, glass-texture and door helpers. Complete arena, Game.data and ordered create, load, virtual, mesh, texture and sector effects compare. These effects remain controlled library boundaries. Nine additional door-sign inputs and four separate Vehicle GameDone wheel inputs are reported separately. Sparse DoLights and nested unadmitted helpers add no entry credit. |
| Game bounded embedded Vehicle GameDone | [crates/lhr-engine/src/car_game_done_vehicle.rs](../crates/lhr-engine/src/car_game_done_vehicle.rs) | 1 | 6 | No integration recorded | Six unique core direct Vehicle GameDone prestates compare complete arena, Game.data and ordered controlled resource effects, including reached extinguished-light cleanup. Four additional wheel-cleanup prestates are separate breadth evidence. Illuminated DoLights paths, actual library resource backends and whole lifecycle ownership are excluded; no separate DoLights credit. |
| Game original car level tables and packed parameter conversion | [crates/lhr-engine/src/car_game_init_levels.rs](../crates/lhr-engine/src/car_game_init_levels.rs) | 2 | 39 | Static inspection | Fourteen archive-aware LoadLevelTables ABI prestates and twenty-five direct packed-converter prestates compare original results, complete admitted heap and ordered file and allocator effects. Original filenames, row order, packed copies, padding and partial writes are retained. Native file and allocator operations are controlled boundaries. A separate original-DTA runtime adapter passes eight configurations in both profiles; this does not complete Car GameInit. |
| Game bounded vehicle Handbrake setter | [crates/lhr-engine/src/vehicle_controls.rs](../crates/lhr-engine/src/vehicle_controls.rs) | 1 | 73 | No integration recorded | Seventy-three unique borrowed-storage SetHandbrake inputs from seventy-four named trajectories compare the complete original 64 KiB state and defined boolean in both profiles. Original ramp, clamp and release behavior is retained. This is an input setter without vehicle dynamics. |
| Game bounded UpdateSteeringWheels | [crates/lhr-engine/src/vehicle_wheel_pose.rs](../crates/lhr-engine/src/vehicle_wheel_pose.rs) | 1 | 31 | Static inspection | Thirty-one unique original UpdateSteeringWheels inputs compare complete 64 KiB state with explicit quaternion and frame effects. Original-data A and D preview publishes recovered steerable-wheel quaternions through the same initialized ModelArena and graph update. Body motion, wheel rolling, road contacts and playable driving are outside that preview. |
| Game bounded UpdateRotationWheels | [crates/lhr-engine/src/vehicle_wheel_pose.rs](../crates/lhr-engine/src/vehicle_wheel_pose.rs) | 1 | 36 | No integration recorded | Thirty-six unique original UpdateRotationWheels prestates from thirty-eight named trajectories compare complete 64 KiB state and bounded required quaternion and frame effects. Actual rolling publication during body simulation is not established by the steering preview. |
| Game bounded Vehicle Engine scheduler | [crates/lhr-engine/src/vehicle_dynamics.rs](../crates/lhr-engine/src/vehicle_dynamics.rs) | 1 | 147 | No integration recorded | One hundred forty-seven unique full entry capsules from one hundred sixty-two named Engine trajectories compare complete 64 KiB state, defined boolean and ordered controlled Move and AngleTo effects. Borrowed and addressable API reruns add no input credit. Actual retained AngleTo and logical precision-history composition are separate checked cohorts; no complete solver or driving claim follows from scheduler parity. |
| Game bounded composed Vehicle Move | [crates/lhr-engine/src/vehicle_dynamics_move.rs](../crates/lhr-engine/src/vehicle_dynamics_move.rs) | 1 | 79 | No integration recorded | Seventy-nine unique full entry capsules from eighty-one named complete Move trajectories compose fifty zero-wheel and twenty-nine nonzero-wheel inputs. All internal preparation, force, wheel, rigid-body and tail phases share one address map and add no canonical entries. Complete state, defined return and ordered required effects compare with controlled body, world and library dependencies. Original road driving and mission integration remain unavailable. |
| Game bounded Vehicle Tick modes zero one and two | [crates/lhr-engine/src/vehicle_dynamics_tick.rs](../crates/lhr-engine/src/vehicle_dynamics_tick.rs) | 1 | 64 | No integration recorded | Sixty-four unique full entry capsules from sixty-six named admitted Tick trajectories compare complete 64 KiB state and ordered drivetrain, Engine and effect boundaries for modes zero, one and two. Six original mode-three undefined-local trajectories are excluded. Six additional uphill cases are separate breadth evidence; profile or API repetitions add no cases. No live game caller or driving pipeline is claimed. |
| Game bounded Vehicle Update orchestration | [crates/lhr-engine/src/vehicle_dynamics_update.rs](../crates/lhr-engine/src/vehicle_dynamics_update.rs) | 1 | 52 | No integration recorded | Fifty-two unique full Update prestates compare complete 64 KiB state, defined result and ordered model, scene, sound, dynamic-registration, particle and wheel-publication effects. Unsigned millisecond conversion, original material-table and RNG order are retained. Wheel steering, rotation, axes and other external dependencies remain controlled in this corpus. This is not complete game-loop or driving integration. |
| Game bounded DoWheelsCollision | [crates/lhr-engine/src/vehicle_contacts.rs](../crates/lhr-engine/src/vehicle_contacts.rs) | 1 | 84 | No integration recorded | Eighty-four unique original inputs comprise fifty-eight complete DoWheelsCollision returns and twenty-six placement-frontier prefixes. Twelve full-return impact cases prove that two original callback arguments alias one mutable impulse, that callback and persistent mass mutations affect the later damage threshold, and that particles, sound, callback and damage effects retain their original order. Earlier accumulated velocity deltas retain their pre-callback impulse. Complete and prefix observations remain separate. World and effect callees are controlled boundaries; dirty-frame and actual loaded-road ownership are outside this corpus. No driving validation is established. |
| Game bounded BodyCollision traversal and fine-hit admission | [crates/lhr-engine/src/vehicle_dynamics_body_collision.rs](../crates/lhr-engine/src/vehicle_dynamics_body_collision.rs) | 1 | 8 | No integration recorded | Eight unique original BodyCollision inputs compare complete arena, selected Game.data scratch globals, defined result and ordered scratch, filter and sphere-query effects. Empty, coarse-miss and bounded coarse and fine hit admission are exercised with controlled query results and initialized scratch capacity. Static impulse and damage callbacks, descriptor mapper mutation, nested dynamic owners and actual loaded-world queries are outside this eight-case proof. |
| LS3DF bounded S_vector AngleTo | [crates/lhr-engine/src/native_vector_angle.rs](../crates/lhr-engine/src/native_vector_angle.rs) | 1 | 202 | No integration recorded | Two hundred two unique direct original S_vector AngleTo inputs compare the admitted PC24 nearest finite-normal or zero result and preserved state. Retained extended tokens, CRT precision-dependent sine products and quotient continuation have separate composition cohorts. Logical precision-history scope is required for admitted composition; unscoped negative controls intentionally differ. Architecture-specific environment preservation is scoped to the isolated assembly block. |
| Game bounded SphereInFace and SphereInEdge | [crates/lhr-engine/src/collision_swept.rs](../crates/lhr-engine/src/collision_swept.rs) | 2 | 944 | No integration recorded | Nine hundred forty-four distinct original direct-entry capsules comprise seven hundred sixty-four Face and one hundred eighty Edge cases. Initial overlap, one-sided planes, ordered edge and nominated-vertex paths, strict distance and resolved sphere center retain original arithmetic. Complete state and defined boolean compare; finite normal or zero PC24 domain limits remain explicit. |
| Game bounded SphereInNonface and SphereInAABox | [crates/lhr-engine/src/collision_swept_world.rs](../crates/lhr-engine/src/collision_swept_world.rs) | 2 | 926 | No integration recorded | Nine hundred twenty-six distinct original direct-entry capsules comprise five hundred ninety Nonface and three hundred thirty-six AABox inputs. Sphere, infinite cylinder, selected AABox faces, transformed OBB and XTOBB paths preserve native edge, vertex and transform order. Source planes and local radius are retained. Complete state and defined boolean compare within the bounded numeric domain; no actor or dynamic-world owner is invented. |
| Game bounded static TestSphereB worker and tree traversal | [crates/lhr-engine/src/collision_swept_world.rs](../crates/lhr-engine/src/collision_swept_world.rs) | 2 | 172 | No integration recorded | One hundred seventy-two unique full capsules from one hundred seventy-three named direct _TestSphereB requests execute the actual nested SphereInTreeLevel body. Complete state, original primitive identity, strict nearest distance, masks and optional output preservation compare. A separate native PC marker confirms Tree reachability; there are no standalone Tree entry prestates. Dynamic-grid presence and reached missing geometry remain typed unavailable. TestSphereB thunk and host-world adapter gain no credit. |
| Game bounded DBox collision worker and dependencies | [crates/lhr-engine/src/collision_swept_box.rs](../crates/lhr-engine/src/collision_swept_box.rs) | 4 | 77 | No integration recorded | Seventy-seven distinct original direct-entry inputs comprise twenty-eight _TestDBox, fifteen BoxInTreeLevel, seventeen BoxInNonFace and seventeen BoxInDynamic cases. Defined boolean results agree and original source memory remains read-only. Original tangent rules, stored squared extents, ordering and required owner availability are retained. Actual live dynamic actor registration and original-world driving are not established. |
| LS3DF Scene weather reset and sector sound defaults | [crates/lhr-engine/src/scene_weather.rs](../crates/lhr-engine/src/scene_weather.rs) | 2 | 800 | No integration recorded | Eight hundred direct original-instruction inputs comprise 736 WeatherSystemReset and 64 sector sound defaults inputs, including incoming precision P0/P1. Full 65536-byte fixture arenas and logical sticky P match actual Cargo debug/release; native O0/O3 wrappers add no cases. Original fixed-angle math executes without numeric mocks. Unknown Scene words and signed zeros are retained. Scene construction, shared Snow ownership, full Close, driver/global ownership and mission integration remain open. |

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
| `RUST-SCOPED-PORTS-20261022-06` | 2026-10-22 | Game.exe 4783 / 4,914 (97.33%); rw_data.dll 6 / N/A; LS3DF.dll 1530 / 1,689 (90.59%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 46 | 0/12 |
| `RUST-SCOPED-PORTS-20261022-07` | 2026-10-22 | Game.exe 4783 / 4,914 (97.33%); rw_data.dll 6 / N/A; LS3DF.dll 1530 / 1,689 (90.59%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 68 | 0/12 |
| `RUST-SCOPED-PORTS-20261022-08` | 2026-10-22 | Game.exe 4783 / 4,914 (97.33%); rw_data.dll 6 / N/A; LS3DF.dll 1530 / 1,689 (90.59%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 72 | 0/12 |
| `RUST-SCOPED-PORTS-20261022-09` | 2026-10-22 | Game.exe 4783 / 4,914 (97.33%); rw_data.dll 6 / N/A; LS3DF.dll 1530 / 1,689 (90.59%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 83 | 0/12 |
| `RUST-SCOPED-PORTS-20261022-10` | 2026-10-22 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 95 | 0/12 |
| `RUST-SCOPED-PORTS-20261022-11` | 2026-10-22 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 110 | 0/12 |
| `RUST-SCOPED-PORTS-20261022-12` | 2026-10-22 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 117 | 0/12 |
| `RUST-SCOPED-PORTS-20261022-13` | 2026-10-22 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 126 | 0/12 |
| `RUST-SCOPED-PORTS-20261022-14` | 2026-10-22 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 129 | 0/12 |
| `RUST-SCOPED-PORTS-20261022-15` | 2026-10-22 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 147 | 0/12 |
| `RUST-SCOPED-PORTS-20261022-16` | 2026-10-22 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 147 | 0/12 |
| `RUST-SCOPED-PORTS-20261022-17` | 2026-10-22 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 151 | 0/12 |
| `RUST-SCOPED-PORTS-20261022-18` | 2026-10-22 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 159 | 0/12 |
| `RUST-SCOPED-PORTS-20261022-19` | 2026-10-22 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 160 | 0/12 |
| `RUST-SCOPED-PORTS-20261022-20` | 2026-10-22 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 161 | 0/12 |
| `RUST-SCOPED-PORTS-20261023-21` | 2026-10-23 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 162 | 0/12 |
| `RUST-SCOPED-PORTS-20261023-22` | 2026-10-23 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 163 | 0/12 |
| `RUST-SCOPED-PORTS-20261023-23` | 2026-10-23 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 164 | 0/12 |
| `RUST-SCOPED-PORTS-20261023-24` | 2026-10-23 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 165 | 0/12 |
| `RUST-SCOPED-PORTS-20261023-25` | 2026-10-23 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 167 | 0/12 |
| `RUST-SCOPED-PORTS-20261023-26` | 2026-10-23 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 168 | 0/12 |
| `RUST-SCOPED-PORTS-20261023-27` | 2026-10-23 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 168 | 0/12 |
| `RUST-SCOPED-PORTS-20261023-28` | 2026-10-23 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 176 | 0/12 |
| `RUST-SCOPED-PORTS-20261023-29` | 2026-10-23 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 177 | 0/12 |
| `RUST-SCOPED-PORTS-20261023-30` | 2026-10-23 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 179 | 0/12 |
| `RUST-SCOPED-PORTS-20261023-31` | 2026-10-23 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 181 | 0/12 |
| `RUST-SCOPED-PORTS-20261023-32` | 2026-10-23 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 192 | 0/12 |
| `RUST-SCOPED-PORTS-20261023-33` | 2026-10-23 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 195 | 0/12 |
| `RUST-SCOPED-PORTS-20261023-34` | 2026-10-23 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 204 | 0/12 |
| `RUST-SCOPED-PORTS-20261023-35` | 2026-10-23 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 214 | 0/12 |
| `RUST-SCOPED-PORTS-20261023-36` | 2026-10-23 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 219 | 0/12 |
| `RUST-SCOPED-PORTS-20261023-37` | 2026-10-23 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 222 | 0/12 |
| `RUST-SCOPED-PORTS-20261023-38` | 2026-10-23 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 227 | 0/12 |
| `RUST-SCOPED-PORTS-20261023-39` | 2026-10-23 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 245 | 0/12 |
| `RUST-SCOPED-PORTS-20261023-40` | 2026-10-23 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 255 | 0/12 |
| `RUST-SCOPED-PORTS-20261023-41` | 2026-10-23 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 257 | 0/12 |
| `RUST-SCOPED-PORTS-20261023-42` | 2026-10-23 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 264 | 0/12 |
| `RUST-SCOPED-PORTS-20261024-43` | 2026-10-24 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 271 | 0/12 |
| `RUST-SCOPED-PORTS-20261024-44` | 2026-10-24 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 272 | 0/12 |
| `RUST-SCOPED-PORTS-20261024-45` | 2026-10-24 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 309 | 0/12 |
| `RUST-SCOPED-PORTS-20261024-46` | 2026-10-24 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 309 | 0/12 |
| `RUST-SCOPED-PORTS-20261025-47` | 2026-10-25 | Game.exe 4784 / 4,914 (97.35%); rw_data.dll 176 / 234 (75.21%); LS3DF.dll 1555 / 1,714 (90.72%); Setup.exe (supplied configuration) 172 / 172 (100.00%) | 311 | 0/12 |

Regenerate locally with `python3 scripts/progress.py --write`; verify without writes with `python3 scripts/progress.py --check`. Versioned snapshots are retained in `docs/progress.json`.
See the [README overview](../README.md) for the compact dashboard.
Supplied binary catalog (file identities; independent measures):

| Binary | Platform | Variant | Role | Static retained pseudocode | Scoped Rust entries |
| --- | --- | --- | --- | ---: | ---: |
| DrvMgt.dll | PC | Installed game | Driver-management helper, exact runtime use unresolved | N/A | 0 |
| Game.exe | PC | Installed game | Game and mission executable | 4784 / 4,914 (97.35%) | 149 |
| IJoy.dll | PC | Installed game | DirectInput joystick wrapper | N/A | 0 |
| INet.dll | PC | Installed game | Network helper, exact runtime use unresolved | N/A | 0 |
| LS3DF.dll | PC | Installed game | 3D, animation, audio and rendering engine | 1555 / 1,714 (90.72%) | 145 |
| Setup.exe (supplied configuration) | PC | Installed game | Installed game configuration utility | 172 / 172 (100.00%) | 5 |
| binkw32.dll | PC | Installed game | Bink video codec boundary | N/A | 0 |
| ogg.dll | PC | Installed game | Ogg container library boundary | N/A | 0 |
| rw_data.dll | PC | Installed game | Game archive and data access library | 176 / 234 (75.21%) | 12 |
| vorbis.dll | PC | Installed game | Vorbis codec library boundary | N/A | 0 |
| vorbisfile.dll | PC | Installed game | Vorbis stream and file library boundary | N/A | 0 |
| Setup.exe (archive installer bootstrap) | PC | Distribution archive | Distribution installer bootstrap | 376 / 376 (100.00%) | 0 |
| SLUS_206.71 | PS2 | PS2 disc executable | PS2 game executable with retained linker symbols | N/A | 0 |
| CDVDSTM.IRX | PS2 | PS2 disc IOP module | CD and DVD streaming support module | N/A | 0 |
| IOPRP271.IMG | PS2 | PS2 disc support image | IOP reboot and support image, container contents unmeasured | N/A | 0 |
| LIBSD.IRX | PS2 | PS2 disc IOP module | Sound support module | N/A | 0 |
| MCMAN.IRX | PS2 | PS2 disc IOP module | Memory-card management module | N/A | 0 |
| MCSERV.IRX | PS2 | PS2 disc IOP module | Memory-card service module | N/A | 0 |
| MTAPMAN.IRX | PS2 | PS2 disc IOP module | Multitap support module | N/A | 0 |
| PADMAN.IRX | PS2 | PS2 disc IOP module | Controller support module | N/A | 0 |
| SDRDRV.IRX | PS2 | PS2 disc IOP module | Sound driver module | N/A | 0 |
| SIO2MAN.IRX | PS2 | PS2 disc IOP module | SIO2 support module | N/A | 0 |
| SOUND.IRX | PS2 | PS2 disc IOP module | Game-distributed sound module | N/A | 0 |
| SOUND2.IRX | PS2 | PS2 disc IOP module | Game-distributed sound module | N/A | 0 |
| SOUND_M.IRX | PS2 | PS2 disc IOP module | Game-distributed sound module | N/A | 0 |

N/A means no measured static procedure ratio. PS2 linker symbol research supplies naming evidence, not static procedure coverage. Scoped entries are absolute reviewed counts joined by binary SHA-256; zero is not 0% coverage. Whole-function and differential runtime coverage remain unmeasured (N/A).
<!-- generated-function-progress:end -->

</details>
``````````

<a id="remediation"></a>

## docs/implementation/mafia-re-remediation.md

``````````markdown
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
``````````
