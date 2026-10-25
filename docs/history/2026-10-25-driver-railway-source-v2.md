# Driver, railway and existing-Scene LoadMap — historical Source-v2

Previous current summaries at baseline `d915f656` retain their original source and
validation scopes. They are historical, not latest proof.

## README.md at baseline d915f656

The bounded CPU lifecycle now uses the original driver type-13 Scene factory,
constructor viewport override and borrowed scene registry, with explicit registry
storage cleanup. Native audio-driver behavior and full driver/device shutdown
remain outside the admitted domain.

The ordered actor loader now admits kind-8 `C_railway` construction, existing
base Init, guarded LoadData and destruction. Model StopAnimation follows the
actual null-IPolSet guard; railway GameInit remains unavailable. An
`OriginalLoadMap` initializer enumerates a genuine existing Scene and publishes
its primary/backdrop/omni sector references. It does not supply full mission
creation, top objects, model cache, player/camera ownership or authored script
scheduling.

Accepted code: `b62dd2c`; [source-bound validation](../../COVERAGE.md).


## COVERAGE.md at baseline d915f656

## Current bounded milestone

The actual driver type-13 factory now constructs the Scene, applies its original
viewport setter and appends a borrowed registry pointer without AddRef. Scene
destruction removes that pointer; driver member-storage cleanup frees retained
registry capacity after all Scenes are gone. This does not establish full driver
shutdown, native sound-listener behavior or exceptional-unwind equivalence.

Kind-8 `C_railway` supports constructor defaults, existing base Init, guarded
25-byte LoadData and destructor cleanup. The actual model StopAnimation guard
reads `model+0x150`; the admitted null IPolSet performs no controller writes.
Controller ownership remains untouched. Railway GameInit/GameDone and
their gameplay allocations remain unimplemented.

The existing-Scene `OriginalLoadMap` initializer performs actual frame enumeration
and publishes existing primary/backdrop/omni references, excluding Near. It creates
no sectors. Callers still supply a genuine Scene; complete mission
construction, top-object loading, model cache and Mission Open remain open.
Player/camera ownership, authored actor GameInit and script scheduling are absent.

Earlier immediate texture/cache support remains bounded to its documented domain.
Legacy model textures and canonical weather textures are separate owners. Original
data has no Snow texture, preserving the native null outcome. CPU Close still
reports `native_device_virtual14_executed=false`; no complete native Close, device
or GPU parity is claimed. The script probe remains FINDACTOR/UnsupportedWorldBackend,
PC 3 / source line 4. All six first-mission gates remain open.

Code `b62dd2c` has two sequential independent non-author source approvals.
The full gate passes 1113 Rust tests, 9 REA-wrapper and 47 progress tests,
plus formatting, locked checking, Clippy and the application build.
Debug UI checks cover three reloads and four zero-owned CPU/driver cleanups.
Physical CPU comparisons match 10 driver and 25 railway cases in both Cargo
profiles; O0/O3 agree. Six native-only railway frontiers are excluded.
Separately, qemu/RSP observes 8 world cases / 36 publications; four debug Rust
world tests include eight schedule observations, without physical/dual-profile parity.
Driver member-storage cleanup has primary/unit/UI evidence, not native comparison.

[Detailed source-bound counts and limits](../progress.json).


## docs/PROGRESS.md at baseline d915f656

## Current milestone — 2026-10-25

The bounded CPU lifecycle now uses the original driver type-13 Scene factory,
constructor viewport override and borrowed scene registry, with explicit registry
storage cleanup. Native audio-driver behavior and full driver/device shutdown
remain outside the admitted domain.

The ordered actor loader now admits kind-8 `C_railway` construction, existing
base Init, guarded LoadData and destruction. Model StopAnimation follows the
actual null-IPolSet guard; railway GameInit remains unavailable. An
`OriginalLoadMap` initializer enumerates a genuine existing Scene and publishes
its primary/backdrop/omni sector references. It does not supply full mission
creation, top objects, model cache, player/camera ownership or authored script
scheduling.

Immediate textures and constructor-owned Scene cleanup remain the preceding
bounded milestone. Device14 is still explicitly unexecuted in the CPU projection;
legacy model and weather texture ownership are separate. Missing original Snow
stays null. Full Mission Open and first-mission play remain unavailable; the script
probe remains FINDACTOR/UnsupportedWorldBackend at PC 3 / source line 4.

Code `b62dd2c` has two sequential independent non-author source approvals.
The full gate passes 1113 Rust tests, 9 REA-wrapper and 47 progress tests,
plus formatting, locked checking, Clippy and the application build.
Debug UI checks cover three reloads and four zero-owned CPU/driver cleanups.
Physical CPU comparisons match 10 driver and 25 railway cases in both Cargo
profiles; O0/O3 agree. Six native-only railway frontiers are excluded.
Separately, qemu/RSP observes 8 world cases / 36 publications; four debug Rust
world tests include eight schedule observations, without physical/dual-profile parity.
Driver member-storage cleanup has primary/unit/UI evidence, not native comparison.

[Detailed source-bound counts and limits](../progress.json).
