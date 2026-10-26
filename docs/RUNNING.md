# Build and run

The program reads a legally obtained original PC installation through `--root`.
Keep game files outside this repository. Rust 1.99.1 and `Cargo.lock` are pinned.
The original executable and reverse-engineering tools are not runtime inputs.
The menu and mission commands inspect current resources; the first mission is not
playable from start to finish.

## Linux — tested with Vulkan

Install a C compiler and linker, `pkg-config`, ALSA development headers, a
Vulkan loader and driver, X11 or XWayland, Python 3 and `curl`. On Debian,
`build-essential pkg-config libasound2-dev` supplies the build-side packages;
install a Vulkan driver and X11 runtime suitable for your machine. The project
uses X11 through winit; its Wayland backend is not enabled.

For a Debian desktop, one package example is:

```sh
sudo apt update
sudo apt install -y build-essential pkg-config libasound2-dev libvulkan1 mesa-vulkan-drivers xorg python3 curl
```

Choose the Vulkan driver and display server appropriate for your machine.
From the repository root:

```sh
./scripts/setup.sh
./scripts/test.sh
./scripts/run.sh --root /path/to/Mafia
./scripts/run.sh --root /path/to/Mafia --model models/taxi00.4ds --car-driving
./scripts/run.sh --root /path/to/Mafia --mission mise01 --headless --capture /tmp/mise01.png
./scripts/run.sh --root /path/to/Mafia --mission mise01 --mission-load-probe --probe-model-dir Models
source scripts/common.sh
cargo run --locked -p lhr-engine --features engine-lab -- --root /path/to/Mafia --model models/taxi00.4ds --car-driving --lab
```

`setup.sh` downloads the pinned Rust toolchain and locked crates; it does not
install system build packages. The optional `./scripts/setup.sh --graphics`
installs a local software Vulkan runtime only on Debian 13 x86_64. Set
`LHR_SOFTWARE_VULKAN=1` to select that installed runtime. A headless capture
still needs a working Vulkan device.

Driving opens a window: W/S accelerate and brake, A/D steer, Space applies the
handbrake, R reloads, and Escape exits. `--mission mise01` captures an
inspection view; it does not start authored mission progression. To capture a
bounded window run, add `--frames 300 --capture /tmp/driving.png` to a windowed
command. `--car-driving` cannot be combined with `--headless`. The optional Engine Lab
needs the `engine-lab` feature and a window; F1 toggles its panel, F2 pauses,
F3 steps and F4 reloads. It is disabled in the default build. The CPU
`--mission-load-probe` runs before the window/audio device path and does not
execute gameplay. Pass `--probe-model-dir Models` explicitly for model intake;
its intentional default is an empty directory list, so a run without that
argument proves no model geometry.

## Windows — native build and run not yet tested

Install the official Rust 1.99.1 MSVC toolchain, Visual Studio Build Tools with
the C++ workload and Windows SDK, and a Vulkan-capable graphics driver. Use
PowerShell in the repository root. The Bash setup scripts are not native Windows
commands. The renderer selects Vulkan, not DirectX 12.

```powershell
rustup toolchain install 1.99.1 --profile minimal --component rustfmt --component clippy
cargo +1.99.1 fetch --locked
cargo +1.99.1 build --workspace --locked
cargo +1.99.1 run --locked -p lhr-engine -- --root "C:\Games\Mafia"
cargo +1.99.1 run --locked -p lhr-engine -- --root "C:\Games\Mafia" --model models/taxi00.4ds --car-driving
cargo +1.99.1 run --locked -p lhr-engine -- --root "C:\Games\Mafia" --mission mise01 --mission-load-probe --probe-model-dir Models
cargo +1.99.1 run --locked -p lhr-engine --features engine-lab -- --root "C:\Games\Mafia" --model models/taxi00.4ds --car-driving --lab
```

These commands follow the current CLI and dependency configuration; no native
Windows build, GPU, audio or interactive result has been recorded. `cargo
+1.99.1 test --workspace --locked` runs the Rust tests, but does not replace
all checks in `scripts/test.sh`.

## macOS — native build and run not yet tested

Install Xcode Command Line Tools, Python 3 and `curl`. `setup.sh` accepts
Apple Silicon and Intel Macs. The selected graphics backend is Metal; audio
uses CoreAudio.

```sh
xcode-select --install
./scripts/setup.sh
source scripts/common.sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 -m unittest discover -s tests -p 'test_progress.py'
python3 scripts/progress.py --check
./scripts/run.sh --root /path/to/Mafia
./scripts/run.sh --root /path/to/Mafia --model models/taxi00.4ds --car-driving
./scripts/run.sh --root /path/to/Mafia --mission mise01 --mission-load-probe --probe-model-dir Models
cargo run --locked -p lhr-engine --features engine-lab -- --root /path/to/Mafia --model models/taxi00.4ds --car-driving --lab
```

Native macOS build, Metal rendering, audio and interaction have not been run.
The Linux-to-Apple cross-check stopped at missing CoreAudio SDK headers; that
cross-check does not establish a native macOS result. `--graphics` is a
Debian-only setup option. The full `scripts/test.sh` also exercises the Linux
analysis wrapper, which requires `/proc`, CPU affinity and `prctl`; use the
portable checks listed above on macOS.
