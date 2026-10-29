# Reference-project audit

## Latest pinned source comparison — 2026-10-29

The existing private cache was fetched and its master reference pinned to
[`dc867c3238488de27d094bb79b6550e392566e3c`](https://github.com/Marvisak/mafia-re/tree/dc867c3238488de27d094bb79b6550e392566e3c).
The comparison inventories all 111 changed paths and inspects selected diffs
and source bodies from the two-commit delta after `eb5a921`. Game Init/Done bodies
now provide dependency maps; the main loop, mission Tick, parameter decoding,
actor and road dependencies remain incomplete. The Rust PR integration adds
bounded compiler/navigation/loading dependencies, without claiming full parity.

[Executed integration and fresh PC REA evidence](implementation/2026-10-29-pr-integration.md)
separate source comparison, native instruction observations, PS2 symbol naming,
Rust original-data probes and actual captures. The new driver-volume analysis
does not yet provide a production master-volume operation or original-runtime
comparison. Earlier records retain their original dates and tool limitations.

## Retained upstream source comparison — 2026-10-28

Fresh GitHub commit and file reads reached [mafia-re
`dc867c3238488de27d094bb79b6550e392566e3c`](https://github.com/Marvisak/mafia-re/tree/dc867c3238488de27d094bb79b6550e392566e3c),
two commits beyond `eb5a921`. This is a pinned source comparison, not a fresh
full checkout: the execution host is disconnected and local `git clone` failed
DNS. Rust integration baseline remains `1d2fd3b` from the record below.

| Area | Newly available source | Still not established |
| --- | --- | --- |
| `game/mission/game.cpp` | Bodies for `Init`, `Done`, `DoneIntern` and many dependencies reached by the menu. | Upstream says those three bodies are not matched yet; their presence is not runtime equivalence or our integration. |
| `game/mission/C_program.cpp` | `Init`, `Done`, `Compile`, `CallSubroutine`, `CallProcess` and keyword decoding. | `DecodeParams` and `Process` are still stubs; a complete original script cannot run from this source alone. |
| `game/ai/pathweb.cpp` | `DeRiciConns` clears the two evidenced flag bits on nodes and connections. | `LoadNodes` remains a stub; the installed `check.bin` stream still needs its actual loader. |
| `game/ai/jp_ai_entity.cpp` | No new constructor implementation. | `C_entity` is still a stub; actor 27 cannot be admitted as a generic actor. |
| `game/mission/Mission.cpp` | More lifecycle and named-program orchestration. | `Tick` remains a stub; ordered loading is not mission execution. |

The new Rust `program_compiler` module implements bounded line-scan orchestration
with the existing keyword recognizer and an explicit instruction-owner/parameter-
decoder boundary. Its only concrete backend here is a synthetic test backend.
It is not called by `CProgram::Init`, does not supply `DecodeParams` or a VM, and
does not change the first-mission frontier. No accepted-function credit is added.

Next functional work: implement evidenced parameter decoding and payload
ownership, compose program Init/Done with the actual source owner, then connect
the new GameInit/Done sequence in dependency order. Keep C_entity reconstruction
and production navigation intake explicit parallel dependencies. Preserve typed
live state rather than importing the reference's native pointer/layout spelling.
Earlier evidence below keeps its original revision and scope; in particular,
the old statements that GameInit/Done have no bodies are historical, not current.

## Retained source refresh — 2026-10-28

The live upstream head is now [mafia-re
`dc867c3238488de27d094bb79b6550e392566e3c`](https://github.com/Marvisak/mafia-re/tree/dc867c3238488de27d094bb79b6550e392566e3c),
two commits after the integration reference below. Fresh pinned source reads
show new Game Init/Done, compiler-front-end and navigation-reset bodies;
DecodeParams, Process, C_entity construction and LoadNodes still have stubs.
The new Game lifecycle bodies are explicitly not matched yet upstream.

[Updated dependency order and this change's exact verification limits](implementation/2026-10-28-upstream-refresh.md)
replace the old upstream-status assumptions, not the dated runtime evidence.
A full new local checkout was attempted but not obtained: the execution host
is disconnected and download DNS is unavailable. Do not label these API reads
as a completed clone or as original-runtime validation.

## Validated integration baseline — 2026-10-28

The integration checkout was pinned to [mafia-re
`eb5a9216eea23210e96a3148e351e8211b27e240`](https://github.com/Marvisak/mafia-re/tree/eb5a9216eea23210e96a3148e351e8211b27e240).
Sound destruction and reciprocal sector-list ordering were compared with its
LS3DF source during the independent audio reviews. Its vehicle-sound file does
not supply a GearSnd body, so it does not independently validate that function.

The supplied PC Game.exe, LS3DF.dll and rw_data.dll hashes match the versioned
binary catalog. Fresh PS2 ELF metadata again matches the recorded boot binary,
15,236 symbol entries and 7,788 function symbols; its actual
`GearSnd__9C_VehicleFv` symbol is at `0x0026b900`, size 412 bytes. This is symbol
metadata, not a new REA analysis, a PC layout proof or an emulator execution.

[The integration record](implementation/2026-10-28-pr-integration.md) separates
Rust tests, GPU checks, original-data loading, native UI/driving and the remaining
menu/mission limits. No original-executable differential comparison or new parity
credit follows from these checks.

## Retained source and tool check — 2026-10-27

The current parity stack is pinned to [mafia-re
`eb5a9216eea23210e96a3148e351e8211b27e240`](https://github.com/Marvisak/mafia-re/tree/eb5a9216eea23210e96a3148e351e8211b27e240).
Sound/render contracts, mission source order, menu navigation, material clocks,
MRPH, startup patches and navigation ownership are reviewed against that
checkout and separate PC REA evidence. The original campaign is still not
playable; the reference's GameInit/Done and road-reader stubs remain dependencies.

New local PS2 intake found 15,236 ELF symbol entries, including 7,788 function
symbols, in `SLUS_206.71` SHA-256
`adf4944c5cdac23fcc4e9af9ef3ec1ba6234497e57cec2af2ff1a7a15c62a46f`.
The symbols corroborate original navigation, MRPH and program names, without
establishing PC layout or platform-equivalent behavior. REA 6.7.0 commit
`6d356b84344659085f2b912857b5e27fe0160c9b` with Ghidra 12.1.4 refuses this
R5900 target before analysis. No provider check was bypassed and no original
executable was run. [Current evidence and exact limits](implementation/parity-continuation.md)
separate source comparison, original-data compatibility, static metadata and
runtime validation. Earlier revisions below retain their original scope.

## Retained source and tool check — 2026-10-26

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

## Retained connected Rust scope — 2026-10-26

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

[Current implementation and validation](implementation/parity-continuation.md) ·
[remaining goals](GOALS.md) · [coverage](../COVERAGE.md) · [history](history/README.md).
