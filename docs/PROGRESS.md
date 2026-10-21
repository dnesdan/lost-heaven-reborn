# Progress

Updated 2026-10-21 UTC. The project has reset from a Godot gameplay prototype to a standalone Rust/wgpu engine and game. The first two milestones add a bounded reader for loose files and selected PC DTA archives, plus approximate model inspection rendering. Full campaign completion remains the goal.

| Measure | Current status |
| --- | --- |
| Rust workspace | Latest `scripts/test.sh` passed 27 Rust tests (21 assets, 3 engine, 3 renderer), 9 REA wrapper tests, formatting, workspace check and Clippy. Two sequential independent reviews accepted the DTA milestone against manifest `bb397fe59de45cb471c07a00f227e6a111f8ecc7cf9f1c9c5a766464962b6d7c`. |
| Asset reader | Reads supported loose files and A1/A2/A6 DTA archives. Private validation compared all 20,229 indexed resources (1,393,380,816 bytes) against a separate private prototype decoder. This is not an independent source extraction. Four known resources also match independent loose-source hashes. |
| Linux renderer | Vulkan llvmpipe offscreen captures for 4build101 and bedna00 were read back and inspected. Archive-path captures exactly match their previous loose-file PNG hashes. Xvfb/XTEST verified camera-inspection input; this is not game-control validation. Rendering is approximate, without original-engine visual parity. |
| Archive limits | Audio codecs and checksum verification are unsupported. Archive precedence is a project loose-first policy; original game mount/order behavior is not established. |
| macOS / Metal | Toolchain selection is prepared, but no Mac validation has been performed. |
| Original gameplay | Not implemented or validated. The inspection camera is not original-game behavior. |
| Campaign | Not playable through the Rust engine. |

Historical private REA/Ghidra counts are 9/4,914 Game.exe procedures in the default inventory (0.18%) and 45/1,683 LS3DF.dll procedures in an explicitly documented union profile (2.67%). These are retained static pseudocode counts, not understood or implemented behavior. The strict full-system evidence gate remains 0/12 subsystems; no overall remake-completion percentage is defined. Bounded static contracts `PC-RUST-BOOTSTRAP-001` and `PC-RUST-ARCHIVE-001` record selected boot, format, scene-tick, bitmap, archive and lookup findings; they do not establish the full game loop or runtime fidelity. One private runtime trace measured 40 animation API time units as 40 milliseconds on a specific unscaled path, not a universal clock rule. See [COVERAGE.md](../COVERAGE.md) for boundaries.

## Reset boundary

The former Godot prototype, including its synthetic gameplay checks and private renders, is historical. Its implementation and tests are not counted as Rust features or validation. Useful format, binary and original-runtime evidence remains in private files. Public tests use synthetic fixtures.

## History

| Revision | Milestone | Verification and analysis snapshot |
| --- | --- | --- |
| [`a077ef2`](https://github.com/dnesdan/lost-heaven-reborn/commit/a077ef25e887b6129c893daf9e5c4885904a2c31) | Started the standalone Rust engine and original loose-model renderer. | 13 Rust tests and 9 REA wrapper tests passed. Game.exe 9/4,914 (0.18%); LS3DF.dll 45/1,683 (2.67%); strict full-system evidence 0/12. |

## Verification

Linux x86_64 uses Rust 1.99.1. The latest `scripts/test.sh` completed formatting, `cargo check --workspace --all-targets --locked`, workspace tests, Clippy and all 9 REA wrapper tests. Private archive validation streamed all 20,229 indexed resources and found exact byte equality with a separate private prototype decoder; four known files match independently recorded loose-source hashes. Two bounded archive-path rendered PNGs match the prior loose-path captures. These checks establish reader and renderer-path consistency, not checksum assurance, full-world loading, original-render equivalence or gameplay. Task-owned processes were stopped. Native macOS/Metal remains untested.
