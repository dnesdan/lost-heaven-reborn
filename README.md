# Lost Heaven Reborn

<!-- generated-function-progress:start -->
As of 2026-10-22 · snapshot `RUST-SCOPED-PORTS-20261022-09`.

<p align="center"><img src="docs/progress.svg" alt="Progress dashboard: static retained pseudocode, reviewed scoped Rust entries, and strict system verification" width="100%"></p>

| Original binary | Static retained pseudocode | Denominator basis | Full contracts | Full Rust ports | Differential runtime verified |
| --- | ---: | --- | ---: | ---: | ---: |
| Game.exe | 4783 / 4,914 (97.33%) | Raw profile inventory | N/A | N/A | N/A |
| rw_data.dll | 6 / N/A | Unknown inventory | N/A | N/A | N/A |
| LS3DF.dll | 1530 / 1,689 (90.59%) | Explicit profile union | N/A | N/A | N/A |
| Setup.exe (supplied configuration) | 172 / 172 (100.00%) | In-image non-thunk procedures | N/A | N/A | N/A |

Strict full-system gate: **0/12**. Binary-level static counts use the named denominator basis and do not combine into a game-completion percentage. Unknown denominators remain N/A. 1 supplied configuration Setup inventory uses 172 in-image non-thunk procedures. Separately, archive installer bootstrap static inventory: **376 / 376 (100.00%)**; this distribution target is not combined with engine-library counts. The campaign is not playable.

Accepted scoped Rust ports: **83 entries across 32 reviewed contracts**. These remain separate from retained pseudocode and whole-function counts.

Regenerate locally with `python3 scripts/progress.py --write`; verify without writes with `python3 scripts/progress.py --check`. Versioned snapshots are retained in `docs/progress.json`.
Milestone history and scoped implementation evidence are in [docs/PROGRESS.md](docs/PROGRESS.md).
<!-- generated-function-progress:end -->

Static percentages use the last audited inventory. Scoped function ports and actual runtime integration are tracked separately; newer research receives no credit until reviewed.

An independent Rust reimplementation of the Mafia: The City of Lost Heaven (2002) engine and game. The current build reads a user's original PC installation, displays the Czech PC menu layout and assets, renders bounded scenes, and plays a verified single-root position animation from unchanged original model/animation files. MISE01 inspection assembles a small neighborhood around the authored player spawn with the original static taxi. It does not execute the cinematic, missions, or gameplay. The long-term goal is a complete campaign using the user's own legally obtained game data. No runtime game data is included.

Original skinned models now have a bounded static pose path: owned Joint hierarchy, joint binding, bounds and CPU deformation feed the renderer. `civil02` matches the original DLL across all 3,124 compared vertex, matrix, flag and bounds values. This does not yet animate characters or make Tommy playable.

The immediate goal is **the playable first mission, from the original menu through mission completion**, alongside function-by-function library parity with mafia-re. See the [acceptance gates and dependency order](docs/GOALS.md). PS2 symbol names guide reconstruction; PC behavior and platform differences require original-binary verification.

| Czech PC menu in Rust | MISE01 static inspection in Rust |
| --- | --- |
| ![Rust engine showing the Czech menu](docs/screenshots/2026-10-21-menu.webp) | ![Rust engine showing the static MISE01 neighborhood and taxi](docs/screenshots/2026-10-21-mise01.webp) |

[Dated screenshot history](docs/screenshots/README.md) · 176 KiB total for these two previews. The mission view is not playable.

![Original civil02 character in a static pose through the recovered skin pipeline](docs/screenshots/2026-10-22-original-skin.webp)

Actual Rust capture using original geometry and textures; first LOD, static pose and inspection lighting. [Evidence and reproduction](docs/screenshots/README.md#2026-10-22--original-skinned-character).

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
./scripts/run.sh --root /path/to/Mafia --model models/4build81.4ds
./scripts/run.sh --root /path/to/Mafia --model models/civil02.4ds
./scripts/run.sh --root /path/to/Mafia --model models/sipka1.4ds --animation models/sipka1.5ds
```

The first command opens the Rust-rendered original menu. Up/Down and Enter select options; only New Game and Exit work, and there is no profile chooser. New Game switches to a static intro scene; Escape switches to the bounded MISE01 spawn-neighborhood inspection. This temporary inspection path does not play the original video/cinematic or execute missions or gameplay. Add `--headless --capture /tmp/scene.png` to either inspection command for a one-frame GPU capture, or `--frames N` to bound a window run. Camera inspection uses arrow keys and W/S. See [coverage](COVERAGE.md) for exact evidence and limits.

Standalone static `--model` loading now builds an owned mesh/dummy hierarchy using recovered common LS3DF fields, parent links, reference transitions and guarded original transforms. `4build81.4ds` exercises three meshes and one dummy. Unsupported frame types, world-space visuals, reflective materials and unverified numeric paths report errors. This route does not establish native world streaming or original lighting/material parity. The MISE01 neighborhood remains a separate static inspection path.

The same command admits a bounded original singlemesh/Joint hierarchy and prepares its first-LOD pose without changing the source data. Across all 82 selected paths accepted by the strict skin reader, 80 models produce checked poses with 37,948 vertices; two dog paths fail explicitly outside the supported numeric domain. Only `civil02` has the separate composed original-DLL comparison described above. Morph models, skeletal animation and native vertex-buffer/resource lifetime remain unsupported; recovered normals are not yet used by the unlit renderer.

The animation command supports one matched position-only track and one root standard mesh with verified source flags. It uses recovered clock, model and interpolator behavior together with shared original frame-cache semantics and bounded original matrix/point arithmetic. Camera, handedness conversion, materials and rasterization remain inspection choices. Character animation, rotation/scale tracks, notes and arbitrary animation binding remain unsupported. For repeatable GPU captures, add `--headless --animation-steps 25 --capture /tmp/animation.png`: each controlled inspection step advances 16 ms through the same update path. Use 0, 25 and 50 steps for the loaded state, 400 ms and 800 ms; this is controlled test time, not a claim about original wall-clock presentation.

`setup.sh --graphics` prepares the Linux software Vulkan tools used for development captures. `scripts/test.sh` checks formatting, compilation, tests, Clippy, REA wrappers and generated progress. On Linux x86_64, X11 input has been checked through menu, static INTRO and the MISE01 neighborhood, including orbit and reload. An Apple arm64 cargo check passes, but macOS linking and Metal runtime have not been tested. iOS is a future compatibility target. The original game's executable and REA are not gameplay runtime dependencies.

## Reverse engineering and assets

REA with Ghidra is the required analysis path for engine behavior. Evidenced original function/type names are preserved, with Rust naming conventions; descriptive working names remain explicitly distinct from recovered symbols. Keep original executables, installations, extracted or converted assets, analysis databases, decompiler output, credentials and private URLs outside Git. Public tests use synthetic fixtures. The reader supports selected PC DTA archives and bounded model, scene, menu and script-data formats; archive audio and checksum verification remain unsupported. Full city simulation, mission execution and campaign play are not implemented. Use a trusted, stable installation tree. PS2 material is used for research and comparison.

Project code is MIT licensed. Game assets and trademarks belong to their respective owners. See [third-party notices](THIRD_PARTY_NOTICES.md), the [pinned reference-project audit and PS2 symbol crosscheck](docs/REFERENCE_PROJECTS.md), and the [asset inventory and optional graphics assessment](docs/ASSET_ENHANCEMENT.md).
