# Human actors, owned program execution and recording intake

Tested engine: `041eed8ba03546c945a92dbbf15a33af2e5e0ed2`.
The first mission and all six [gameplay gates](../GOALS.md) remain open.

## Actual actor initialization

The existing ActorWorld now owns genuine Human/Player and `C_entity` state.
Factory kinds 2 and 27 use their original allocation contracts, constructors,
model-only Init, LoadData and derived destruction. Existing kinds 1/5/8 retain
their behavior. Typed allocation leases replace actor byte-buffer backing;
there is one authoritative actor and one actual embedded Entity program.

Human constructors preserve unwritten seeds, the inventory and animation-machine
owners, sixteen original property defaults and the untouched current-property
bank. Player changes its evidenced defaults. Entity publishes its program owner
before Human Init. Repeated live child lookups, original all-type mask `0xffff`,
userdata publication and registration order remain intact.

Versions 5/6 read each complete field separately; a field larger than the
remaining bounded input is skipped without advancing or overwriting it. Entity
continues on that same reader into its ignored derived-version byte and real
CProgram source. Older conversion branches remain unavailable; observed native
default-version success is preserved. Underlying partial DTA I/O is outside this
bounded-memory adapter's equivalence claim.

Cleanup follows derived → program → Human → base order. It stops slot 0 on the
actual attached model, preserves other slots, and prevents retry after uncertain
destructive effects. Unreconstructed populated Human/Entity collections have no
invented producers: this is the constructor-empty source-lifetime domain.

This implements original Init, not the later GameInit, AI, combat, PersonAnim
selection, vehicle attachment or mission scheduling. PS2 symbols corroborate
class names and selected correspondence. PC inventory construction and eight
animation slots are preserved; PS2's initial inventory item and three slots are
platform differences, not PC defaults.

## Actual base CProgram runtime

The same CProgram owns its working source copy, compiled instructions, pending
parameter allocation, PC, elapsed state and actor/frame register arrays. Init
uses the existing original Compile scan and a real integer-parameter decoder;
there is no diagnostic `Program::from_source` substitution.

The scalar parameter layout covers **102 opcode IDs**. Actual owned execution
currently supports **DIM_FRM (11), DIM_ACT (12) and COMMANDBLOCK (34)**. Other
layouts or executable commands stop at explicit reached dependencies. Ordinary
native compile rejections continue as observed; an unavailable compile dependency
retains its prefix and prevents execution.

The supported domain is a fresh **base** CProgram. Entity/Detector virtual
execution, saved-code chains and repeated Init remain unavailable. Done releases
parameters in reverse order and retains instruction capacity. Logical vector
clearing does not drop live parameter owners. A failed destructive callback
blocks Process/Done retries; nondestructive interruptions can still reach Done.
Source destruction requires a separate explicit host discard of retained runtime
storage. That discard is not original Done or destructor equivalence.

## Original recording prefix

The new typed cutscene owner reads the original 96-byte header and resource
dictionary. With `record01a.rep` it retains 28 entries and reaches byte **1,560**,
immediately before allocation of 38 actor descriptors, RVA `0x19a48d`.
It remains **stopped (4)** with unchanged clocks. No descriptor attachment,
record playback, camera/HUD track execution or successful RECLOADFULL is claimed.

Its source is a buffered installation adapter, not original DTA open/read/close
parity. Partial-read availability and interruption are explicit. Consuming host
discard releases the source and dictionary; native Close is a separate gap.
The first later recorded actor is a type-4 car, which needs the genuine recorded
actor/vehicle pipeline.

## Original-data results

| Scene | Objects | Actors / global program sources | CPU result | Disposal |
| --- | ---: | ---: | --- | --- |
| INTRO | 358 | 28 / 15 | Scene2 prefix complete, 434,014 bytes; exit 0 | Complete, tracked owners zero |
| MISE01 | 3,183 | 33 / 0 | Unsupported actor kind 4 at 3,805,059; exit 1 | Complete, tracked owners zero |
| 00menu | 69 | Unchanged frontier | Animated model at 466,049; exit 1 | Interrupted model cache |

Global program-source counts exclude embedded Entity programs. CPU probes ran
on the staged executable, whose full SHA-256 equals the final committed binary;
the equality and source manifests are retained. No authored program ran in these
scene-load probes.

Fresh final-binary GPU captures both exit 0: [INTRO](../screenshots/2026-10-29-human-vm/intro-source.webp)
shows only sky/clouds, and [MISE01](../screenshots/2026-10-29-human-vm/mise01-source.webp)
shows the static street/bridge/construction view. They do not demonstrate mission
actor animation or interactive gameplay. [Sanitized evidence and image hashes](../screenshots/2026-10-29-human-vm/evidence.json).

Menu cleanup still retains 24 frames, 11 cache allocations, 3 graphics,
22 textures and 52 visual allocations until process exit. Completing menu
animation loading remains separate from the new actor and VM work.

## Verification and traceability

Each substantial milestone passed two sequential independent code reviews,
including native evidence, ownership and reached error behavior. Review fixes
preserve uninitialized register tails, move host limits after the original
free/count prefix, reject unsupported derived dispatch and block uncertain-free
replay. Final formatting received independent verification.

On the clean tested commit, `scripts/test.sh` passed **1,881 Rust + 106 Python
tests**. The all-feature run including ignored GPU tests passed **1,905 Rust
tests, zero ignored**; counts overlap. Formatting, check, both Clippy configurations
and the application build passed. Tests introduced here cover synthetic inputs.

Actual public APIs from the sealed final engine library match controlled original
PC instruction results:

| Scope | Comparison | Result |
| --- | --- | --- |
| Human/Player | 4 constructor/fresh-destructor cases + 142 bounded reads; 3,492 normalized words | Exact match in both Rust probe profiles |
| Scalar parameter arm | 1,632 cases across 102 IDs; 900,864 bytes including source mutations and allocation observations | Exact match |
| Fresh base VM | 7 Init → Compile → CallProcess/selected Process → Done cases; 368 words / 1,472 bytes | Exact match |

Both Human Rust probe profiles link the same default debug engine, not two engine
build profiles. VM native tests use constructor-compatible supplied fixtures,
not execution of the original constructor. Human base/frame dependencies and
allocator/free boundaries are controlled; global registration is not compared.
General callback/error cases have separate Rust tests, not complete native
exception or allocator parity. Original expected observations were reused
for the final library comparisons. The full original game was not launched.

PC Game SHA-256:
`08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27`.
Canonical RVAs include Human ctor/Init/LoadData/dtor
`0x16efd0` / `0x16f350` / `0x16f4e0` / `0x16f210`, Player ctor `0x18f170`,
Entity ctor/Init/LoadData/dtor `0x1071a0` / `0x107470` / `0x107e20` / `0x107310`,
CProgram Init/DecodeParams/Compile/CallProcess/Process/Done
`0x61580` / `0x61c70` / `0x6c770` / `0x6ccb0` / `0x6d3b0` / `0x616a0`, and
cutscene constructor/LoadFull/dictionary `0x19cc40` / `0x19a3c0` / `0x19bab0`.
These are scoped provenance mappings, not full-function completion counts.
The separate PS2 naming reference has SHA-256
`adf4944c5cdac23fcc4e9af9ef3ec1ba6234497e57cec2af2ff1a7a15c62a46f`.

Original binaries, assets, decompiler output, databases and raw receipts stay
private. Linux/software Vulkan was exercised; native macOS/Metal and Windows
were not. The accepted scoped snapshot remains **311 entries / 114 contracts**;
strict system verification remains **0/12**, whole-function coverage **N/A**.

The next dependencies are the genuine type-4 car actor and recorded-actor
binding, then remaining GameInit/VM dispatch, recording tracks, human animation
selection and original mission scheduling/objectives. No substitute actor,
automatic idle clip or forced mission advancement bypasses them.
