# Lost Heaven Reborn

An independent Rust reimplementation of the Mafia: The City of Lost Heaven (2002) engine and game. The Rust bootstrap reads a bounded subset of original 4DS models and BMP textures, and has rendered one original building and one crate with wgpu on Linux. This is an initial asset/render milestone, not a playable world or a visual match. The long-term goal is a playable full campaign using a user's own lawful PC installation. No proprietary game files are included. See [coverage](COVERAGE.md) for scope and evidence.

## Build and run

Requires Rust 1.99.1, installed by the setup script. Linux uses wgpu with Vulkan; macOS is intended to use Metal. Rust dependencies are pinned in `Cargo.lock`.

```sh
./scripts/setup.sh
./scripts/test.sh
./scripts/run.sh --synthetic
```

Run from the repository root. `setup.sh --graphics` prepares the verified Linux software Vulkan tools used for development captures. On Linux without a hardware Vulkan driver, the runner selects llvmpipe when the supporting graphics runtime is installed. `run.sh` launches the current Rust engine from source; a packaged standalone game is not available yet. The original game's executable and REA are not runtime dependencies.

To inspect a supported model from a legally obtained installation, provide its extracted loose-file directory and model-relative path. For example, use `./scripts/run.sh --root /private/game-data --model models/4build101.4ds --textures maps --headless --capture /tmp/building.png`. The model and textures are read locally; DTA archives are not searched. `--synthetic` renders the built-in test scene.

The setup and Rust build workflow was exercised on Linux x86_64. Actual offscreen Vulkan rendering was verified with llvmpipe. The scripts include native macOS and Linux arm64 toolchain selection, but macOS/Metal has not been tested. iOS is a future compatibility target.

## Reverse engineering and assets

REA with Ghidra is the required analysis path for engine behavior. Keep original executables, installations, extracted or converted assets, analysis databases, decompiler output, credentials and private URLs outside Git. Public tests use synthetic fixtures. The current reader supports a limited set of loose PC game files; DTA archives, full city loading and gameplay are not supported. The engine is intended to load the user's own legally obtained PC installation. PS2 material is used for research and comparison.

Project code is MIT licensed. Game assets and trademarks belong to their respective owners. See [third-party notices](THIRD_PARTY_NOTICES.md).
