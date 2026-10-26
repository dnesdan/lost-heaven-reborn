# Shared mission load session and probe

`MissionLoadSession` composes the existing CPU world intake, scene name map and
ordered scene2 prefix loader over the same real owners. It is a host loading
adapter. It does not implement native `C_mission::Open`, actor installation,
script execution, objectives or mission completion.

## Run the diagnostic

```sh
./scripts/run.sh --mission-load-probe --root /path/to/Mafia --mission MISE01 --probe-model-dir models
```

The probe runs without a window, renderer or audio output device. It reads the
selected installation through `Installation`, using the host path convention
`missions/NAME/scene.i3d` and `missions/NAME/scene2.bin`. The existing world adapter
performs its evidenced `.i3d` to `.4ds` rewrite. The library itself accepts explicit
input paths and does not special-case MISE01.

Repeat `--probe-model-dir DIRECTORY` to supply the actual model-cache `AddDir`
inputs in order. Omitting it leaves the search list empty; no implicit `Models`
directory is installed. The report prints that choice. A negative native model
cache result can still be part of an admitted completed prefix, so successful
parsing is not a fully loaded mission.

Presentation and playback flags are rejected in this mode, including explicit
defaults such as `--frames 0` or `--textures maps`. Ordinary `--mission NAME`
without `--mission-load-probe` retains its existing static-inspection behavior.

The command chooses and prints an explicit diagnostic profile:

| Input | Host selection |
| --- | --- |
| Screen and viewport | 800 × 600, full rectangle |
| Projection helper state | `global_mode = 0`, `screen_flag_46f4 = false` |
| Native scene sound driver | `Absent` |
| Initial modeled PC24 precision flag | Clear |
| Model search directories | Exact caller list, including an empty list |

These values are probe inputs, not a recovered original GameInit prestate. A
caller with established different prestate can supply it through the library.
The precision owner persists through both load and disposal; this particular
owner does not model all x87 control/status fields.

## Owners and ordering

The session owns one `Installation`, `DriverSceneOwner`, `ModelArena`,
`SceneResources`, scene texture/storage owners, `MissionCaches`, cache memory
and precision state. The image adapter borrows that installation. The car
runtime and this session use the same small screen/image adapter definitions;
moving those adapters adds no new resource domain.

Loading executes these existing operations in order:

1. Add the explicitly requested model search directories.
2. Create the actual scene through its driver and resource owners.
3. Load the world through the existing CPU adapter, retaining its real staging
   model, transferred frames and prepared-sector progress.
4. Publish the scene name map from that same hierarchy.
5. Read the selected scene2 input and execute its existing prefix with the same
   arena, driver, scene resources, model cache and installation.

The first refused operation stops the attempt. The session never scans ahead to
an actor section or replaces an unsupported frame with a substitute. A session
can be entered once. Reentry returns an admission error without overwriting the
first detailed failure or repeating effects.

Read-only getters retain the actual world progress, partially published name
map, scene2 cursor, reached source entry, completed object count, pending
geometry/model/link work and cache state. The probe reports counts and presence
without dumping geometry, pixels or scripts. The success phase is deliberately
named **`PrefixComplete`**.

## Terminal disposal and process result

`discard_terminal` is one explicit host disposal attempt. It invokes genuine
staging disposal, cache destructors, scene release and empty driver-registry
release. Its Sound → Model → Animation cache order is a host choice; it does not
claim original MissionClose or CloseSystem ordering.

An interrupted cache or sector, or an incomplete scene constructor, can refuse disposal. The
session retains that separate failure and does not reset, force-clear or retry
the owner. A completely constructed but unregistered scene uses its real
resource-owner release path. Dropping a Rust value is not reported as a native
destructor or a completed disposal.

Completion requires zero exposed live frame, cache allocation, graphics,
texture, sector, portal, occluder and weather owners. Both actual atlas registries
also contribute their allocation, failed-allocation, atlas and RGBA-byte counts.
An allocation that failed before a ready atlas exists cannot disappear behind
an empty texture count.

The command gathers load evidence, attempts disposal, appends disposal evidence,
then writes the report. An output failure therefore cannot skip the attempted
resource cleanup. The original load failure and any disposal failure remain
separate. Saved frame/scene identifiers become historical after successful
disposal and must not be dereferenced as live handles.

Exit status is zero only when the admitted prefix and genuine terminal disposal
both complete. Every load, disposal or output failure produces a nonzero status.
Every report still states **Gameplay acceptance: NOT EVALUATED**.

## Validation and the first-mission gate

Synthetic integration tests cover real world transfer, name-map publication,
cache loading, absent inputs, partial constructor failures, interrupted cache
cleanup, a late world-Prepare failure after genuine frame transfer, independent
session identities and actual saved-lightmap destruction.
The actor-root regression stops at the existing `0xae20` boundary and preserves
its exact entry/cursor. It does not manufacture an actor or a mission result.
CLI tests reject incompatible modes before opening an installation.

An original installation can stop earlier, including at an unsupported world
visual subtype. The probe reports the first boundary actually reached rather
than asserting that every mission reaches the same one.

The [first-mission acceptance gates](GOALS.md) remain open. The next dependencies
are Game association/replacement of saved `LMAP` visuals, genuine MissionOpen
and GameInit owners, the required actor factories and player/world interfaces,
and the original program/compiler/scheduler and objective effects. The saved
lighting resource and GPU work is documented separately in
[vertex lighting](saved-vertex-lighting.md) and
[rectangle lightmaps](saved-lightmap-atlas.md).

Finishing those dependencies requires the private REA contracts and original
installation/runtime comparisons described in the repository conventions.
Neither a successful synthetic probe nor an isolated driving session closes
the menu → original objectives → completion → repeat/restart acceptance path.
This adapter adds no native-function or system-gate coverage credit.
