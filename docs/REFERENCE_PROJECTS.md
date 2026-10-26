# Reference-project audit

## Current source and tool check — 2026-10-26

Fresh inspection confirms [mafia-re
`29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c`](https://github.com/Marvisak/mafia-re/tree/29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c),
unchanged from the preceding source audit. It supplies substantial LS3DF bodies
for visual factories/copies, saved lighting, camera/render passes, portal and
occluder processing. Its Game mission/startup/actor stubs do not supply a
working campaign. Existing binary-specific contracts and historical behavior
pins retain their own scope.

REA repository source was updated to clean revision `3d565fa42` / version 6.8.
Static analysis used an isolated 6.7 runtime because host Node 24.19 is below
6.8's required 24.22.1. Fresh PC instruction receipts, retained exact-body
crosschecks and actual PS2 symbol evidence are recorded privately. No original
game executable/function or reference application was executed for this work;
REA remains a development tool, not a game-runtime dependency.

## Connected Rust scope

Source [`dddaf539`](https://github.com/dnesdan/lost-heaven-reborn/commit/dddaf539c9b874b9cf7a67d866532d901729df18)
now composes the bounded dependencies through real owners:

| Dependency | Current connected scope | Remaining boundary |
| --- | --- | --- |
| Visual factory/copy and scene2 | Real `OBJ_` → `LMAP` owner replacement, hierarchy/map/refcount effects, saved-payload dispatch | Scene2 stops at 44 records / cursor 18693 on `FRAME_DUMMY` (6), `detector_konec`; complete MissionOpen remains open. |
| Saved lighting | Owned vertex colors/lightmap geometry, UV2, atlas allocation/update and actual GPU sampling | Full Construct, 16-bit/dither/compression/resize and general shader behavior remain open. |
| Scene and Camera | Actual active camera/viewport, backdrop/main/near passes, portal/occluder visibility | Complete native rendering, driver cache/effect paths and original player/camera scheduling remain open. |
| City-cache placements | 907 measured scene-owned model instances sharing real resources, secondary-scale replacement and live visibility correspondence | Existing host-selector admission is explicit; original city streaming/pooling/controller equivalence is unclaimed. |
| Failure/lifetime | Pending creators and interrupted Scene/CarRuntime owners remain inspectable; explicit scene→cache→reclaim cleanup | Native allocator/device/failure parity is not established. |

The current image still has dark or absent road and sky; the cause is unresolved.
CPU/source tests, GPU readback, actual-data probes and interactive UI are separate
proofs. No new scoped entry, whole-function or original-runtime credit follows:
**311 entries / 114 contracts**, strict **0/12**, whole-function **N/A**.

## Other retained references

These earlier source observations were not freshly revalidated in this milestone.

| Reference | Retained revision | Useful scope |
| --- | --- | --- |
| [MafiaUnity](https://github.com/MafiaHub/MafiaUnity/tree/0d7a12f03ce4385173377c3e9f457dcc2e891f24) | `0d7a12f0`, archived | Resource composition, reference joining and patch order; Unity lifetime/prototypes do not establish native ownership or mission behavior. |
| [OpenMF-Archived](https://github.com/MafiaHub/OpenMF-Archived/tree/7167e74d65eeced643aa51fb56bb62b8556d8389) | `7167e74d`, archived | Inspectable scene/name/KLZ relationships; abandoned renderer/physics/player prototypes are source leads. |
| [LS3D-SceneKit](https://github.com/alex-alex/LS3D-SceneKit/tree/8fadd2201f52936d998c66c47b0a0d37476c2ce5) | `8fadd220` | Format and hierarchy/skin leads, without PC layout/ABI/runtime equivalence. |

The [complete predecessor reference audit](history/2026-10-26-scene-render-predecessor.md#reference-projects)
preserves older revisions, source links, naming caveats and format/codec projects
verbatim. Use actual PS2 symbols for original names and PC instructions for PC
behavior; upstream working labels and source folder layout are not original
symbol or library-boundary proof.

<a id="rust-organization-and-original-names"></a>
<a id="current-source-audit--2026-10-25"></a>
<a id="next-general-engine-work"></a>

Earlier inbound audit/name-mapping anchors are retained here. Their original
dated text is in the [complete predecessor reference audit](history/2026-10-26-scene-render-predecessor.md#reference-projects);
the current dependency order is in [goals](GOALS.md).

[Current implementation and validation](implementation/mafia-re-remediation.md) ·
[remaining goals](GOALS.md) · [coverage](../COVERAGE.md) · [history](history/README.md).
