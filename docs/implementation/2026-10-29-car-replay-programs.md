# Car actors, recorded actors and owned global programs

Tested code: `d9c2d3eefecefc2325f9fb6ac0aa1bb840102986`. The frozen reviewed/tested source tree
was committed without code changes; final library and application hashes match
the artifacts used for the original-data and native comparisons.
The observations below identify the tested checkpoint or controlled fixture;
they do not establish a playable campaign or completed original GameInit.

## One mission actor and model owner

Car factory intake uses the actual mission ActorWorld and ModelArena,
with one base actor incarnation and one derived Vehicle/Car state owner.
Its contained Vehicle compatibility storage retains the derived tail rather
than a second base actor image. Standalone CarRuntime is not substituted for
mission ownership, scene ownership or scheduling.

Construction, reached Init, derived destruction and the deleting wrapper share
that ownership. Model identity is checked against the live arena incarnation.
The reached source caller uses the same arena and original installation for
resource lookup. An interrupted operation retains its reached state and cannot
be silently replayed as a fresh successful initialization.
Shared car services retain the original table/cache relationships, one RNG
progression and precision state across cars. Supplied allocator tags are
explicit diagnostic inputs, not values inferred from host pointer addresses.
Precision handling uses the existing owner-scoped PC compatibility boundary.
PS2 symbols provide naming and layout cross-checks, not interchangeable object
layouts: the embedded Vehicle begins at PC car+0x70 versus PS2 car+0x80.
This does not establish a C_public_physics base or justify adding that type.

## Original car data reaches the existing converter

The outer car LoadData path connects actual source records to the COMP decoder,
then the existing conversion and Vehicle parameter application path.
Original taxi and enemy-car inputs each produced 4,385 decoded bytes matching
the independent decoder; original native O0/O2 outputs agreed.
The outer native caller ignores an ordinary inner false result. A host
unavailable/error result remains an interrupted prefix with retained ownership;
it is not converted into native success. Completed calls may receive fresh
source scratch on a later invocation; an interrupted call is not replayed.
Compressed/decoded source leases and parameter scratch have distinct lifetimes.
The original converter leaves string tails and alignment bytes untouched.
The actual Vehicle bank now carries those bytes' validity through opaque
word copies; numeric reads reject unknown bytes instead of falling through to
an unrelated backend. Original taxi and enemy-car banks match every known
native output byte and its mask: 3,140/1,248 and 3,733/655 known/unknown bytes.
A four-byte source word is captured before a potentially overlapping store.

Frame userdata records are independent Arena owners. Their nonreused tokens
resolve through the actual owner and incarnation. Vehicle destruction frees
that record, then clears the frame pointer and releases the model; surviving
aliases become stale. Additional host retirement of remaining record views
requires every actor and frame to be dead. This does not claim native Close.
Init's interrupted Seat/Door/Exhaust local buffers have their own leases;
the car destructor does not silently own or free those caller temporaries.

## Recorded data and the first binding boundary

Recorded input uses one manager-owned serialized pool, with nonowning links
checked against manager identity and generation. Dictionary, descriptor and
camera/record allocations retain their reached order and partial-read prefix.
Explicit host discard is separate from original Close or Clear.
The bounded first-binding path uses the actual actor manager, frame name,
association list and record owner. It does not manufacture a replacement actor
or infer a factory class from the recorded-type selector.
The installed record01a.rep input yielded 28 dictionary entries and 38 descriptors
through the original-path provider. In the explicitly supplied **empty**
ActorWorld/ModelArena fixture, intake stopped at required actor FMVBlackT.
This was not the MISE01 base scene, a LoadDifferences-applied scene, or a proof
of binding that actor in the original mission. Retry was rejected; separate
host cleanup released the retained pool and owned input structures.

## Global programs use their actual source owners

Global Init and Done walk the actual mission program collection in reverse.
Entry count is cached once; each iteration resolves the current member.
They call owned base CProgram Init/Done without cloning source programs.
The base-only runtime guard remains: Entity and Detector virtual decoding and
execution are not silently redirected to the base implementation.
CallSubroutine resets the same program's PC, empty return cursor, owner and
continuation, then synchronously calls its owned Process with delta 1.
Named mission lookup requires a real game-global runner first; an unavailable
game owner is an error, not an invented empty collection or a mission fallback.
These APIs are explicit operations, not automatic scene-load completion.
The Rust runtime currently executes DIM_FRM, DIM_ACT and COMMANDBLOCK only.
It decodes the scalar 102-opcode family and the 39 no-payload opcodes.
A decoded but unsupported command stops when reached; waits are not skipped.
The tested original INTRO session completed source intake with 28 actors and
15 global sources. Explicit Global Init reached reverse index 14, accepted one
record, then stopped at UnsupportedParameters(13), offset 23, in FINDFRAME.
Done rejected the incomplete Init. Explicit host discard released the scalar
and 94-byte working source; terminal disposal reduced all owner counters to zero.
This is a Rust original-content observation, not an original GameInit replay.

## Controlled original-runtime comparisons

| Evidence | Exact scope and limits |
| --- | --- |
| 17,248 car words | Four constructor/null-Init/fresh-destructor fixtures; Vehicle/Car tail projections. Native base ctor/dtor are controlled boundaries. One self identity is normalized after equality checks; allocator tag is supplied. Excludes base-registration equivalence, allocator callback counters, RNG globals/next draw and callback-mutated cohort. Final artifact rerun passed; parameter-bank comparison is a separate original-data proof. |
| 184 binding words | Two preallocated fixtures, four snapshots; original first-binding/reset chain. Actor/record identity normalized after equality checks; pool references use presence/offset. No native constructor/LoadFull allocation, allocator-failure branch, full scene or playback proof. |
| 390 zero-parameter cases | All 39 selectors, original DecodeParams and tokenizer; 215,280 bytes matched. Includes source mutations and allocation absence. No command execution claim. |
| 295 global-loop words | Five supplied constructor-compatible collections, original Init/full Compile/Done chain; 1,180 bytes matched. Allocator/free are controlled; constructors are not executed. |
| 332 subroutine words | Six supplied base-program fixtures, original Init/Compile/CallSubroutine/Process/Done; 1,328 bytes matched. Controlled starting PC/elapsed/continuation; not a mission scheduler run. |
| 668 subroutine words | The same six cases with live PC/elapsed/actor-count/frame-count at each allocator/free boundary; 2,672 bytes matched. This extends the previous projection, not six new independent cases. |

External Rust drivers used actual sealed engine APIs and their sealed dependency
closure; no copied CProgram implementation. Debug/optimized drivers linked the
same engine artifact, not separately built debug/release engines.
Final candidate-v9 API comparisons passed against the immutable original expectations; earlier receipts are retained. The record-destructor comparison adds four final observables, not parity for all 22 free callback events.

## Remaining acceptance work

Original MISE01 loads all 3,183 objects and 38 actors, including both genuine
cars. It stops at unsupported factory kind 20, cursor 3,814,295. Explicit
session disposal completes with all tracked lifetime counters zero, including
car heaps, tables, tracks and Arena records. This is source intake, not driving.
The original menu still stops on its animated model and retains interrupted
cache cleanup. A fresh [MISE01 capture](../screenshots/2026-10-29-car-replay/mise01-source.webp)
exits 0 after canonical cleanup. It shows the static street/bridge view; the
new car actors are not visible in this camera. It proves neither mission
animation nor interactive play. [Sanitized image evidence](../screenshots/2026-10-29-car-replay/evidence.json).

`scripts/test.sh` passed **1,942 Rust and 106 Python tests**. The all-feature
run including ignored GPU tests passed **1,966 Rust tests, zero ignored**;
counts overlap. Formatting, workspace check, both Clippy configurations and the
default application build passed. Both sequential independent reviews cover
traceability, fidelity, aliasing, errors and cleanup. Public fixtures are synthetic.
The existing canonical reload test caught a second host-cleanup path; the final
fix preserves its assertions and retires record views only with no live actors
or frames, before the unchanged strict remaining-owner check.
Car driving/GameInit scheduling, derived program VM execution, replay playback,
first-mission objectives and completion remain outside these bounded proofs.
Accepted scoped totals remain **311 entries / 114 contracts**; strict system
verification remains **0/12**. Whole-function coverage is **N/A** and all six
gameplay acceptance gates remain open. No new parity or gameplay credit is claimed.

## Traceability and next dependencies

PC Game SHA-256:
`08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27`.
Canonical RVAs include Car ctor/Init/LoadData/dtor
`0x1bb80` / `0x1bd80` / `0x1e070` / `0x1bcb0`, Vehicle ctor/Init/source parameters
`0xbd8f0` / `0xbde30` / `0xc60a0`, COMP decode `0x159980`, packed conversion
`0x28380`, recorded attach/manager/reset `0x198fb0` / `0x19d740` / `0x19a240`,
global Init/Done `0x140520` / `0x140550`, and CallSubroutine `0x6cbc0`.
These mappings are scoped provenance, not whole-function completion counts.
REA/Ghidra canonical receipts and unchanged original instruction probes remain
private with their hashes, addresses, confidence and controlled-call boundaries.
Original executables, game files, databases and decompiler output remain
private. The published screenshot is application output.

PS2 naming evidence uses ELF SHA-256
`adf4944c5cdac23fcc4e9af9ef3ec1ba6234497e57cec2af2ff1a7a15c62a46f`.
The configured REA provider still rejects the R5900 analysis target; no new PS2
decompiler or runtime-equivalence claim follows from its retained debug symbols.
Linux/software Vulkan was exercised. Native macOS/Metal and Windows were not.
The complete original game was not launched; native comparisons execute only
the documented controlled original instruction paths.

Next: original kind-20 and remaining source actors, the evidenced 13/18/43
named parameter decoder, actual derived VM/game-global owners and remaining
GameInit order. LoadDifferences(dif01a.chg) must create the genuine recorded
actors before replay; subsequent bindings/tracks, human animation selection,
player/taxi/collision, objectives and restart remain required for the first mission.
