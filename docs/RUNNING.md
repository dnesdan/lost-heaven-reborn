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

## Engine Lab

Build the optional native panel with the same locked dependencies:

```sh
source scripts/common.sh
cargo run --locked -p lhr-engine --features engine-lab -- --synthetic --lab
cargo run --locked -p lhr-engine --features engine-lab -- --synthetic --lab --root /path/to/Mafia
cargo run --locked -p lhr-engine --features engine-lab -- --root /path/to/Mafia --mission mise01 --lab
```

| Tab | Controls |
| --- | --- |
| Scene | Material gallery, outdoor courtyard, geometry playground and light/fog study; searchable installed mission/model catalog; direct relative paths; model, car inspection and driving entry points. |
| Environment | Source reset, daylight/dusk/mist/rain/snow presets; background, sun direction/color/intensity, ambient light, exposure, saturation, distance fog, precipitation and wind; normals and UV views. |
| Camera | FOV, frame subject, six directions, motion speed, auto orbit, one bookmark per loaded scene; chase distance/height/look-ahead for driving. |
| Model | Whole-preview translation, rotation, scale and turntable; select, isolate, hide and frame a material group; reset. Available for procedural scenes and standalone models. |
| Materials | Live animated-material owners, color/environment frame indices, independent clocks, delay/remainder, loop mode and current textures. Reading the panel does not advance lazy getters or consume random values. |
| Vehicle | Keyboard or manual throttle/brake/steering/handbrake, neutral inputs and actual control feedback. Requires driving mode. |
| Diagnostics | Active source, camera pose, draw/triangle/vertex counts, frame interval, actual runtime counters, startup replacement rows, canonical mission intake counts and retained Scene/Car cleanup failures. |

F1 toggles the panel, F2 pauses simulation, F3 dispatches one 16 ms step and F4
reloads. Original scene/model owners retain a driver timeline for animated
materials even when gameplay callbacks are unavailable. Procedural scenes have
no original simulation callbacks. **Animate preview** controls
weather animation, auto orbit and turntable separately; runtime pause also freezes
those previews, with one preview advance per deliberate simulation step.

Outside the panel: arrows orbit, W/S zoom, J/L pan sideways, I/K pan vertically;
right-drag orbits, middle-drag pans and the wheel zooms. Driving retains W/S,
A/D and Space for the vehicle and uses the chase camera. Focus loss clears held
controls and pending steps. Camera bookmarks and draw selections reset when the
source changes. Reload resets scene-local previews and completed-update counts,
while preserving the driver timeline used to admit new material owners. The host
sampling clock is rebased after loading so load duration is not dispatched as a
simulation step. Installed mission names accept the same bounded ASCII letters,
digits, underscores and hyphens as the command-line probe.

Material rows show the last resolved state; a draw must reach its lazy getter
before a texture frame changes. Environment animation state is retained, but
environment-map rendering remains unsupported. Diagnostics distinguish source
intake from authored mission execution and report the exact retained loading
boundary. Inspection never retries or clears an interrupted cleanup owner.

The catalog reads selected archive names and bounded loose directories without
loading every asset. Discovered content can still be unsupported by the original
loader. A failed selection or preset import reports an error and keeps the current
scene/settings; unrecoverable owner cleanup faults remain fatal.

**Portable preset** saves and loads a versioned `.lhr-lab` file. It contains FOV,
chase/navigation parameters, environment and model-preview settings. Camera pose
bookmarks, asset paths, held vehicle inputs and draw indices are not serialized.
Saving uses a temporary file and replacement; importing validates the complete
file before applying it. Local preset files are ignored by Git.

For reproducible scene-only captures (no panel), choose a public scene and an
optional saved preset:

```sh
cargo run --locked -p lhr-engine -- --synthetic --lab-scene courtyard --headless --capture /tmp/courtyard.png
cargo run --locked -p lhr-engine -- --synthetic --lab-scene lights --lab-preset /tmp/mist.lhr-lab --headless --capture /tmp/mist.png
```

Scene names are `gallery`, `courtyard`, `primitives` and `lights`. Without
`--lab-scene`, the original `--synthetic` capture remains the small color quad;
`--synthetic --lab` starts the gallery. Headless previews sample time zero.

Lab lighting uses face normals derived in the shader; it has no shadow maps or
smooth-normal reconstruction. Rain and snow are screen-space visual previews,
without collisions, accumulation or wet-surface physics. Original Scene owners,
saved lighting, camera passes, physics and game assets are unchanged by these
overrides. Canonical scenes and live cars deliberately disable draw-index editing
and model transforms because their visible material groups change with LOD and
visibility. Source restores the loaded appearance; model/camera resets are separate.

After `scripts/test.sh`, check the optional profile explicitly:

```sh
cargo test --workspace --locked --features lhr-engine/engine-lab
cargo clippy --workspace --all-targets --locked --features lhr-engine/engine-lab -- -D warnings
cargo test -p lhr-render --locked -- --ignored --test-threads=1
```

The last command requires a working Vulkan or Metal adapter. Native macOS/Metal
and original installation interaction need separate runs; synthetic checks do not
establish either result.
