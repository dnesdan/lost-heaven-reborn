# Reference-project audit

## Current source and tool check — 2026-10-26

The implementation source cross-check is [mafia-re
`f42a25d9b2cbd674792fe78f5700d5f77b499d0d`](https://github.com/Marvisak/mafia-re/tree/f42a25d9b2cbd674792fe78f5700d5f77b499d0d).
Its LS3DF bodies guide the visual, saved-lighting, Scene, Sound and AREA work;
its Game stubs do not supply a working campaign. Earlier contracts remain
pinned to their own source and binary hashes. Actual PC REA instruction receipts
and PS2 linker symbols supply separate static evidence for behavior and names.
Neither the original Game executable/functions nor the reference application
was run for this milestone. Source names, folders and C aliases do not prove PC
class layout, scalar width or full-function parity.
REA source is clean `3d565fa42` / 6.8; static analysis used isolated
6.7 because host Node 24.19 is below the 24.22.1 minimum.

A final fresh checkout is pinned separately to [mafia-re
`b86ba567`](https://github.com/Marvisak/mafia-re/tree/b86ba567e404e211048fb2bf6eb1a4f133f0fd2f):
14 further commits, 116 files, +6,848/−316 lines. Its delta changes no LS3DF
Scene/model/sky implementation body and reveals no new required fix to the
current admitted object intake. New Game source expands mission Open,
actor/script dispatch, buffered-actor cleanup, menu, HUD and profile evidence;
these remain separate integration dependencies. Existing actor/program
contracts agree within their admitted scope. The original f42 checkout and
its evidence remain unchanged.

The source also separates nonvirtual `C_roads::LoadData` (PC RVA `0x159e60`)
from virtual `MergeData` (`0x15a210`, an upstream working name). Both readers
remain upstream stubs. Neither is the 4DS road geometry already rendered here.

## Connected Rust scope

Current Rust owners cover actual Scene/Camera and saved-lightmap rendering,
corrected road culling, typed Singlemesh/Singlemorph/Billboard/Lensflare
resources, initial animated material pose, and Dummy/Sound/AREA and fog-light
intake. The MISE01 diagnostic reached actor dispatch after all 3,183 objects;
it did not complete mission startup. The optional Lab, standalone driving and
bounded Game control/camera/table/load-save helpers remain separate dependencies. See the
[evidence table](implementation/mafia-re-remediation.md) for receipts and
remaining limits. The accepted ledger and strict gates are in
[coverage](../COVERAGE.md).

## Other retained references

These earlier source observations were not freshly revalidated in this milestone.

| Reference | Retained revision | Useful scope |
| --- | --- | --- |
| [MafiaUnity](https://github.com/MafiaHub/MafiaUnity/tree/0d7a12f03ce4385173377c3e9f457dcc2e891f24) | `0d7a12f0`, archived | Resource composition, reference joining and patch order; Unity lifetime/prototypes do not establish native ownership or mission behavior. |
| [OpenMF-Archived](https://github.com/MafiaHub/OpenMF-Archived/tree/7167e74d65eeced643aa51fb56bb62b8556d8389) | `7167e74d`, archived | Inspectable scene/name/KLZ relationships; abandoned renderer/physics/player prototypes are source leads. |
| [LS3D-SceneKit](https://github.com/alex-alex/LS3D-SceneKit/tree/8fadd2201f52936d998c66c47b0a0d37476c2ce5) | `8fadd220` | Format and hierarchy/skin leads, without PC layout/ABI/runtime equivalence. |

The [complete preceding reference page](history/2026-10-26-pre-final-documentation.md#docsreference_projectsmd)
preserves older revisions, source links, naming caveats and format/codec projects
verbatim. Use actual PS2 symbols for original names and PC instructions for PC
behavior; upstream working labels and source folder layout are not original
symbol or library-boundary proof.

<a id="rust-organization-and-original-names"></a>
<a id="current-source-audit--2026-10-25"></a>
<a id="next-general-engine-work"></a>

Earlier inbound audit/name-mapping anchors are retained here. Their original
dated text is in the [complete preceding reference page](history/2026-10-26-pre-final-documentation.md#docsreference_projectsmd);
the current dependency order is in [goals](GOALS.md).

[Current implementation and validation](implementation/mafia-re-remediation.md) ·
[remaining goals](GOALS.md) · [coverage](../COVERAGE.md) · [history](history/README.md).
