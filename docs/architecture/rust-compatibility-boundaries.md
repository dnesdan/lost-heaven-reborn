# Rust runtime and compatibility boundaries

Source audit: 2026-10-27, integrated code at `6e790279dd5d9bf2a111d0a9330527e8bc4b968a`.
This is an architectural review of the PR stack and the named subsystems below,
not a whole-repository unsafe-code proof or a completed rewrite. A local checkout
scan, compilation, Miri and native platform execution were unavailable.

## Decision

Reproduce observable game behavior, not the original process's memory layout.
Keep actual owned Rust state and typed live views in gameplay. Put serialized
layouts, original-address projections and numeric compatibility behind narrow
adapters. Migrate incrementally against existing behavioral tests; do not replace
an exact arithmetic operation with ordinary `f32` merely because it looks simpler.

Safe references, `Box`, `Rc`, `Arc` and arena keys are not the problem. Removing
them would discard useful lifetime guarantees. Original RVAs in evidence comments,
file chunk IDs, bit masks and GPU/FFI layouts also have legitimate uses.

## Findings and concrete targets

All file links below refer to the existing implementation, not proposed modules.

| Area | Observed implementation | Decision |
| --- | --- | --- |
| [Vehicle owner](../../crates/lhr-engine/src/vehicle_owner.rs) | A `Vec<u8>` models the original object, constructor writes use offsets, a vtable word and the supplied address's high byte. | Replace semantic consumers with typed vehicle state. Keep the original layout/constructor projection only in a compatibility adapter. |
| [Vehicle frame backend](../../crates/lhr-engine/src/vehicle_frame_backend.rs) | Inert address regions such as `FRAME_BASE`, name generations, literal-address mappings and a supplied GameInit stack identify real arena data. | These are not dereferenced host pointers, but they add artificial address-domain limits and process-shaped APIs. Gameplay should use frame/resource IDs and named operations. |
| [Vehicle sound](../../crates/lhr-engine/src/vehicle_sound.rs) | `gear_snd` has named effects, but still reads byte/word offsets and indexes sound banks using raw `u32` handles. | Good first migration: typed live gear-sound accessors and a `SoundId`, preserving rereads after callbacks and shared random state. |
| [Scene loading](../../crates/lhr-engine/src/mission_scene_loading.rs), [audio](../../crates/lhr-engine/src/runtime_audio.rs) | Call sites pass numeric virtual-method slots and bit-pattern arguments through `sound_virtual`. | Move slot translation into one adapter; expose named sound operations to the loader. Keep precise flags and argument-bit semantics. |
| [Model runtime](../../crates/lhr-engine/src/runtime.rs) | Original vtable addresses identify tick behavior, even though actual objects have Rust owners. | Dispatch through a typed class/operation enum or a narrow trait; retain address-to-operation mapping for evidence/replay only. |
| [Navigation](../../crates/lhr-engine/src/mission_web_path.rs) | Owned packed arrays preserve allocation cookies, native pointer words and partial-read effects. Public views already borrow actual storage. | Keep the binary/compatibility decoder, then expose typed nodes/connections. Do not make AI interpret pointer words or recreate the packed allocation layout. |
| [Precision](../../crates/lhr-engine/src/native_precision.rs), [transforms](../../crates/lhr-engine/src/native_transform.rs), [x87](../../crates/lhr-engine/src/native_vector_x87.rs) | Exact software arithmetic exists alongside production x86/x86_64 inline assembly; other targets use separate numeric paths. | Consolidate an explicit numeric policy and portable implementation. Retain x87 as a contained reference/compatibility backend until differential evidence permits moving it out of the default runtime. |
| [Assembly](../../crates/lhr-engine/src/assembly.rs) | An `Arc` pointer value is used for source-identity accounting, not dereferenced through an integer. | Lower priority: use an owner-issued source ID if needed. Do not confuse safe `Arc::ptr_eq` tests with arbitrary memory access. |
| [Morph owner](../../crates/lhr-engine/src/model_morph.rs), [startup leases](../../crates/lhr-engine/src/program_patch.rs) | Immutable definitions are shared while mutable instance state and startup-table lifetime have actual owners. | Preserve this design. Typed shared ownership and read-only inspector snapshots are useful, not debt to remove. |

There is actual `unsafe` assembly in the numeric backend, but many values named
`pointer`, `address` or `vtable` elsewhere are inert compatibility identities.
These are different risks. This audit does not establish that every unsafe block
is sound, nor that all numeric address projections are necessary.

## Proposed boundaries

These are design targets, not newly implemented crates:

- **Formats:** bounded little-endian reads, raw source-name bytes, chunk IDs and
  packed input records. Never turn an integer from an asset into a host pointer.
- **Compatibility adapters:** original allocation identities, layout offsets,
  virtual-slot mappings and exact binary-contract diagnostics. No duplicate
  authoritative world. Borrow real owners when an operation needs live state.
- **Engine/gameplay:** typed actors, scenes, vehicles, resources and ordered
  effects. No CPU-register names, simulated stack addresses or vtable offsets
  in ordinary operation signatures.
- **Numeric compatibility:** documented precision/rounding/operation-order policy,
  portable arithmetic, and a contained x87 reference backend where still required.
- **Presentation/platform:** wgpu layouts, audio/window FFI and bounded read-only
  observations. Host previews must remain distinct from original game behavior.

Keep an evidenced name when known; otherwise choose a clearly descriptive host
name. Do not invent original C++ field semantics to make a struct look complete.
Unknown bytes can remain opaque inside the compatibility boundary.

### Typed APIs without losing live behavior

Prefer `audio.set_volume(sound_id, volume)` to `sound_virtual(raw, 0x68, words)`;
prefer `vehicle.current_gear_sound()` to `word(vehicle, bank + word(vehicle, 0x200)*4)`.
These are proposed API shapes, not a claim that those methods already exist.

A typed accessor must still read the actual owner at the original point in the
operation. If a callback can change the selected sound or shared volume, reread
both after that callback. Do not replace offset access with a detached struct
snapshot and accidentally change aliasing or effect order.

Use owner-scoped IDs with stale-handle checks. Existing nonreused arena keys do
not automatically need generations; add generations when slots can be reused.
A script VM's own float/frame/actor registers may be legitimate language state;
they are not the same thing as emulating x86 general-purpose registers.

### Numeric migration

The x87 routines save/restore processor state and use original precision-sensitive
operations. Removing them without a replacement can change transforms, collision
branches and animation. A portable replacement needs vectors covering rounding,
signed zero, normal/subnormal boundaries, near-singular matrices, normalization,
trigonometric endpoints and failure domains. Preserve error behavior until the
supported domain is deliberately extended with evidence.

Do not promise cross-platform bit identity from ordinary `f64`, platform `acos`,
or a new math library. First define what is exact and what, if anything, permits
a tolerance. Differential tests must compare returned bits, observable side
effects and reached errors, not only visually similar screenshots.

## Migration order and acceptance

1. Inventory actual unsafe/assembly/FFI separately from inert addresses and safe
   references. Classify each hit as test, format, platform, numeric compatibility
   or gameplay. Record the baseline before adding lints.
2. Replace loader sound-slot calls and GearSnd offset reads with typed live APIs.
   Compare call order, flags, callback mutations, PCM output and cleanup against
   the existing adapter. Keep the old path as a test oracle, not a second world.
3. Replace address-based model dispatch and vehicle member access one operation
   family at a time. Keep the public render/physics owner unchanged; verify
   contact, drivetrain, damage and audio traces across repeated ticks.
4. Add typed navigation views/records and production stream binding. Test counts,
   connections, malformed/partial input and interrupted cleanup before AI uses it.
5. Consolidate portable numeric behavior, test x86_64 and native macOS ARM64, then
   move proven-unnecessary assembly behind an explicitly opt-in reference feature.
6. Apply `forbid(unsafe_code)` only to established pure semantic modules. Review
   remaining unsafe blocks for ownership, alignment, bounds, aliasing, ABI and
   state restoration. Run Miri only on its supported pure-Rust subset.

Avoid a repository-wide mechanical rewrite: the existing alias/failure-prefix
contracts are valuable. Address-shaped adapters can be retired after their last
semantic consumer migrates, not before.

A local review can start with the following triage commands; these were **not run
against a full checkout in this audit** and matches require human classification:

```sh
rg -n 'unsafe|asm!|global_asm!|transmute|from_raw|\*const|\*mut' crates --glob '*.rs'
rg -n 'sound_virtual|VTableId|native_this|game_init_stack|FRAME_BASE' crates --glob '*.rs'
```

See [the parity roadmap](../implementation/parity-roadmap-main.md) for the gameplay
dependencies that should advance alongside this migration.
