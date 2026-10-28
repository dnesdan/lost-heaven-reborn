# Rust integration and executed validation — 2026-10-28

PRs #44–48 are merged into `rust-engine`, the sole integration/default branch.
The engine, gameplay and renderer remain in Rust. Godot work belongs in a
separate repository. No CI, runtime dependency, original asset or parity credit
was added by this integration.

The final tested source is `c60abc133fc789502ac0a1acd301088ee56bea0c`; its complete
tree equals merged integration `fec33b59688760f9fd33c02060a5d13c9a750b50`.
Each final code milestone received two sequential independent Sol reviews;
author self-reviews were not counted. Revisions after fixes were reviewed again.

| PR | Result | Final reviewed head |
| --- | --- | --- |
| #47 | Seed actual high x87 instruction addresses in preservation fixtures; keep all prior assertions. | `493b748623bc871ed14de9e7ef2208197a38692a` |
| #44 | Borrow live GearSnd storage with named operations and unchanged callback/error order. | `1e7834d5219fea21845ba18e5af0ba28aa31c8ec` |
| #45 | Retain scene sound incarnations through sector links and teardown. | `2c58146675b29c52037138ce08bbcf73080e7296` |
| #46 | Run the real renderer with bounded process cleanup and raw-source/image receipts. | `7ff1d31fe05f851fa4ceed59c715b06b43b8eda0` |
| #48 | Keep scene-record setters bound to the factory-issued sound identity across resource callbacks. | `c60abc133fc789502ac0a1acd301088ee56bea0c` |

Validation found and fixed two issues before merging. The capture runner had
trusted Git status even though clean filters can hide changed working bytes;
it now hashes raw tracked files and has before/during-capture regressions.
The old scene-close retry fixture recreated an integer handle after release,
which correctly fails the new incarnation guard. It now restores the exact
original test state/generation, retains all prior partial-prefix and successful
retry assertions, and separately checks that stale retries never touch a
replacement. Production identity guards were preserved. Pinned rustfmt was
also applied to the uploaded Rust changes.

## Executed build and logic checks

Linux x86_64, Rust 1.99.1, locked dependencies:

```sh
./scripts/test.sh
cargo test --locked --workspace --all-features
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features --lib --bins --tests -- --ignored --test-threads=1
```

The final default gate passed 1,695 Rust tests and 106 Python tests, including
28 capture-runner regressions. The all-feature run passed 1,698 Rust tests.
All 21 opt-in GPU tests then passed explicitly on Mesa 25.0.7 llvmpipe Vulkan
(LLVM 19.1.7). These default/all-feature counts overlap; GPU tests are additional.
Formatting, workspace check, warnings-denied Clippy and progress verification
passed. On #47, both changed x87 fixtures passed individually; an additional
engine-package opt-level-2 preservation subset passed 168 tests.

## Actual original-data and interactive checks

The supplied Game.exe, LS3DF.dll and rw_data.dll hashes match the versioned
catalog. Original inputs, archives, executables, URLs, passwords, PCM recordings
and raw logs remain outside Git. No original executable was run.

```sh
cargo run --locked -p lhr-engine -- --root /path/to/Mafia --mission mise01 --mission-load-probe --probe-model-dir Models
cargo run --locked -p lhr-engine -- --root /path/to/Mafia --mission mise01 --headless --capture /tmp/mise01.png
cargo run --locked -p lhr-engine --features engine-lab -- --synthetic --lab
```

- MISE01 probe: all 3,183 scene objects and 27 actors reached the known
  unsupported actor kind 27 at byte 3,795,696. The probe reports this frontier
  with exit 1. Terminal disposal completed and every tracked owner was zero.
- MISE01 render: 267 selected material groups, actual source scene/sky and GPU
  readback; exit 0. This is static inspection, not MissionOpen or objective play.
- Menu render: a valid image was produced, but an animated-model admission
  frontier at byte 466,049 left Model cache cleanup interrupted; exit 1. The
  responsible parser/cache/inspection-owner code is unchanged from `172f1c9`.
  A baseline executable was not rerun. The retained owner was not discarded or
  its guard cleared, so this capture is not a green menu-runtime gate.
- Native Lab: gallery-to-courtyard selection was visibly exercised on Xvfb;
  F2/F3 were sent, and Escape exited 0. Public scenes have no original gameplay
  simulation, so key delivery is not a mission pause/step acceptance claim.
- Native original-data taxi: W/A/Space affected actual telemetry, including
  throttle 1, steer -1 and handbrake force 1,200.0001. Escape exited 0 and tracked
  car/model/scene resources were zero. Cold debug initialization exceeded the
  first 30-second wait; a 120-second window wait succeeded.
- Driving mixed 27,648 stereo PCM frames at 48 kHz with nonzero samples. The
  local recording was checked, but physical device output was unavailable.
  This establishes mixing and ownership behavior, not audible/native parity.

## Reference and PS2 checks

A fresh `mafia-re` checkout is pinned to
`eb5a9216eea23210e96a3148e351e8211b27e240`. Independent review compared LS3DF
sound destruction and reciprocal sector-list order. Its vehicle-sound source
does not contain a GearSnd implementation, so source comparison does not supply
an independent GearSnd behavior oracle.

The newly extracted PS2 `SLUS_206.71` hash exactly matches
`adf4944c5cdac23fcc4e9af9ef3ec1ba6234497e57cec2af2ff1a7a15c62a46f`.
ELF metadata contains 15,236 symbols, including 7,788 function symbols;
`GearSnd__9C_VehicleFv` is a function at `0x0026b900`, size 412 bytes.
This repeats static naming evidence only. There was no new REA/decompilation
analysis, PS2 emulation or original-runtime differential comparison.

## Captures and branch cleanup

[Eight fresh images and sanitized receipts](../screenshots/README.md#2026-10-28--reviewed-integration-and-fresh-original-data-checks)
separate four time-zero public scenes, native Lab UI, menu inspection, MISE01
inspection and standalone driving. Every published image was visually inspected.
Exact PNGs remain private where previews use WebP; dimensions are unchanged.

Fifty obsolete remote branches, including `main`, were removed after ancestry
review and a verified Git-bundle backup. The closed hyphenated-mission branch
was a duplicate of the stronger bounded shared validator already integrated;
its old commit is retained in that backup. Only `rust-engine` remained before
the documentation publication branch, which is removed after its merge.

Authored first-mission play and completion remain open. Sound source
Open/reopen/duplicate, resource callbacks, interrupted animated-model loading,
remaining actor dependencies and navigation still require their own evidenced
contracts and integration work. These checks do not change the strict system
gates or the static/scoped-port dashboard.
