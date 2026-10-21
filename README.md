# Lost Heaven Reborn

<!-- generated-function-progress:start -->
As of 2026-10-21 · snapshot `RUST-SCOPED-PORTS-20261021-02`.

<p align="center"><img src="docs/progress.svg" alt="Progress dashboard: static retained pseudocode, reviewed scoped Rust entries, and strict system verification" width="100%"></p>

| Original binary | Retained pseudocode | Full contracts | Full Rust ports | Differential runtime verified |
| --- | ---: | ---: | ---: | ---: |
| Game.exe | 55 / 4,914 (1.12%) | N/A | N/A | N/A |
| rw_data.dll | 6 / N/A | N/A | N/A | N/A |
| LS3DF.dll | 48 / 1,683 (2.85%) | N/A | N/A | N/A |

Strict full-system gate: **0/12**. These static activity ratios are not game-completion percentages. N/A means the whole-function acceptance audit is incomplete; rw_data has no frozen inventory denominator. LS3DF uses an explicit profile union. Accepted stages use absolute counts with an N/A denominator until eligible original functions are audited; raw analyzer inventories include externals and thunks. Full semantic-contract counts are unmeasured. Verified code-byte coverage is unmeasured. The campaign is not playable.

Regenerate locally with `python3 scripts/progress.py --write`; verify without writes with `python3 scripts/progress.py --check`. Versioned snapshots are retained in `docs/progress.json`; milestone history is maintained in `docs/PROGRESS.md`.

Accepted scoped Rust ports: **6 original entries across 5 reviewed contracts**. These preserve the stated domains and do not increase the whole-function figures above. Gameplay integration is pending.

| Rust contract | Original entries | Native-derived validation cases | Verified scope |
| --- | ---: | ---: | --- |
| [Clock reset and elapsed time](crates/lhr-engine/src/clock.rs) | 2 | 20 cases | Serialized shared u32 clock with the standard sampling/sleep backend; no arbitrary IGraph overrides or caller lifecycle. |
| [Scene callback dispatch](crates/lhr-engine/src/scene_tick.rs) | 1 | 13 cases | Immediate mutations and reentry in valid logical collections; animation and actor lifetime remain backend responsibilities. |
| [Interpolator callback control](crates/lhr-engine/src/interpolator.rs) | 1 | 31 cases | Caller control: 23 callback-observer and 8 scalar projections; evaluator math, poses and full note streams remain backend responsibilities. |
| [Game frame-delta modifier](crates/lhr-engine/src/frame_delta.rs) | 1 | 22576 cases | Typed state with masked x87 precision and rounding; original-body comparison excludes FPU status flags, traps and gameplay integration. |
| [Generic actor callback dispatch](crates/lhr-engine/src/actor_tick.rs) | 1 | 26 cases | Visibility and live virtual-call ordering with mutation and reentry; callback implementations, actor ownership and gameplay integration remain open. |
<!-- generated-function-progress:end -->

Static percentages use the last audited inventory; newer private research awaits a separate recount. Scoped Rust ports are tracked independently.

An independent Rust reimplementation of the Mafia: The City of Lost Heaven (2002) engine and game. The current build reads a user's original PC installation, displays the Czech PC menu layout and assets, and renders bounded static scenes. MISE01 inspection now assembles a small neighborhood around the authored player spawn with the original static taxi. It does not execute the cinematic, missions, or gameplay. The long-term goal is a complete campaign using the user's own legally obtained game data. No runtime game data is included.

| Czech PC menu in Rust | MISE01 static inspection in Rust |
| --- | --- |
| ![Rust engine showing the Czech menu](docs/screenshots/2026-10-21-menu.webp) | ![Rust engine showing the static MISE01 neighborhood and taxi](docs/screenshots/2026-10-21-mise01.webp) |

[Dated screenshot history](docs/screenshots/README.md) · 176 KiB total for these two previews. The mission view is not playable.

## Build and run

Requires Rust 1.99.1, installed by the setup script. Linux uses wgpu with Vulkan; macOS is intended to use Metal. Dependencies are pinned in `Cargo.lock`.

```sh
./scripts/setup.sh
./scripts/test.sh
./scripts/run.sh --synthetic
```

Build the native engine binary with `cargo build --locked -p lhr-engine` from the repository root. These commands use the user's game installation directly:

```sh
./scripts/run.sh --root /path/to/Mafia
./scripts/run.sh --root /path/to/Mafia --mission mise01
./scripts/run.sh --root /path/to/Mafia --model models/4build101.4ds
```

The first command opens the Rust-rendered original menu. Up/Down and Enter select options; only New Game and Exit work, and there is no profile chooser. New Game switches to a static intro scene; Escape switches to the bounded MISE01 spawn-neighborhood inspection. This temporary inspection path does not play the original video/cinematic or execute missions or gameplay. Add `--headless --capture /tmp/scene.png` to either inspection command for a one-frame GPU capture, or `--frames N` to bound a window run. Camera inspection uses arrow keys and W/S. See [coverage](COVERAGE.md) for exact evidence and limits.

`setup.sh --graphics` prepares the Linux software Vulkan tools used for development captures. `scripts/test.sh` checks formatting, compilation, tests, Clippy, REA wrappers and generated progress. On Linux x86_64, X11 input has been checked through menu, static INTRO and the MISE01 neighborhood, including orbit and reload. An Apple arm64 cargo check passes, but macOS linking and Metal runtime have not been tested. iOS is a future compatibility target. The original game's executable and REA are not gameplay runtime dependencies.

## Reverse engineering and assets

REA with Ghidra is the required analysis path for engine behavior. Keep original executables, installations, extracted or converted assets, analysis databases, decompiler output, credentials and private URLs outside Git. Public tests use synthetic fixtures. The reader supports selected PC DTA archives and bounded model, scene, menu and script-data formats; archive audio and checksum verification remain unsupported. Full city simulation, mission execution and campaign play are not implemented. Use a trusted, stable installation tree. PS2 material is used for research and comparison.

Project code is MIT licensed. Game assets and trademarks belong to their respective owners. See [third-party notices](THIRD_PARTY_NOTICES.md).
