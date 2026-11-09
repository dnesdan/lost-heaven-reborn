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
./scripts/run.sh --root /path/to/Mafia --model models/TommyTAXIDRIVER.4ds --animation anims/breath01a.5ds --skeletal-animation --headless --animation-steps 100 --capture /tmp/tommy.png
./scripts/run.sh --root /path/to/Mafia --mission mise01 --headless --capture /tmp/mise01.png
./scripts/run.sh --root /path/to/Mafia --mission mise01 --mission-load-probe --probe-model-dir Models
./scripts/run.sh --root /path/to/Mafia --mission intro --mission-load-probe --probe-model-dir Models
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

The live mission owner now binds the `C_mission::Open` mute/restore effects
and the first `Tick` restore-volume gate to the shared Rust PCM master gain.
`skip_frames` delays that one-shot Tick effect; an interrupted audio write
retains the already-consumed flag. This is bounded host audio behavior, not a
complete mission Tick, native DirectSound volume curve or playable GameInit.

The `C_game::Init` ResetMissionState prefix now executes the first scalar
stores, reads weather ON/COUNT from the current retained Scene, and stages
the weather-buffer reset before the host sound mute. It stops explicitly at
the unimplemented scene Render effect; an Open-only input probe lacks a
constructed Scene and stops earlier at that missing owner. Later Init and
script scheduling remain unavailable, with no additional function credit.
The CPU load probe now also prints the first reached `C_game::Init` driver
boundary and the actual retained scalar/weather/PCM observations. This
ordered inspection deliberately runs after the separate `GameInitStart`
script diagnostic; the two independent probes do not establish an executable
native gameplay sequence.
The retained mission also exposes an explicit `SceneRenderBackend` callback
for that same first `C_game::Init` render effect. When supplied, it delegates
through `ModelArena::scene_render_with_backend` with the live Scene identity;
only after a completed callback does it mark the pending volume restoration
and reach the next unavailable `stop_sounds` operation. The default CPU probe
has no GPU renderer and continues to stop at `Scene::Render`.
The load probe also attempts one CPU presentation snapshot from the same
retained `MissionLoadSession` Scene, camera and visibility owner. It reports
the resulting mesh and pass counts, or the first missing render dependency,
before terminal disposal. This is not a GPU readback or a playable frame.
To verify the *same* Scene on the GPU, add `--probe-render-capture FILE.png`
to `--mission-load-probe`. This explicitly requests an offscreen 800x600
readback with the source mesh lighting, and reports GPU errors separately
from input and disposal failures. Store captures outside the source tree;
the source textures and original game data are not redistributable.

Historically at engine `041eed8`, INTRO's CPU probe completed source loading and
exited 0, while MISE01's CPU probe intentionally exited 1 at unsupported car
actor kind 4, cursor 3,805,059; both disposed all tracked owners, without
running GameInit or mission scripts. At tested base `9345c77`, both INTRO and
MISE01 CPU probes with `--probe-model-dir Models` report `PrefixComplete` and
return 0: INTRO reaches 358 objects / 28 actors, MISE01 reaches 3183 objects /
293 actors, and both release all tracked owners at disposal. This covers only
the admitted loading prefix including a `GameInitStart` prefix; it does not
complete MissionOpen, later GameInit, mission scheduling, objectives, or
gameplay. The corresponding INTRO headless GPU source view
currently shows only sky/clouds, without recorded camera playback.

The owned base VM and recording-prefix regressions use synthetic inputs:

```sh
source scripts/common.sh
cargo test --locked -p lhr-engine --lib original_program::owned_runtime
cargo test --locked -p lhr-engine --lib cutscene_owner
```

These exercise the actual owner APIs; there is no public authored mission-play
command yet. [Supported commands, original comparisons and gaps](implementation/2026-10-29-human-program-runtime.md).

The explicit skeletal command is an existing host inspection path, separate
from the new model-owned controller and mission actors. Its 100 controlled
steps sample 1,600 ms; this command and the zero-step baseline were rendered
successfully at `b3b6b67`, changing 18,563 pixels. Omit `--headless`,
`--animation-steps` and `--capture` to open the animated model in a window;
this batch verified the headless command, not the window controls.

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

These commands follow the current CLI and dependency configuration. First
recorded native Windows results at `5d22891` (Rust 1.99.1 MSVC + VS Build
Tools, RX 6700 XT): `cargo check`/`cargo test`/`cargo build` pass, the MISE01
CPU `--mission-load-probe` exits 0 (3183 objects / 293 actors, PrefixComplete),
and the headless captures render (construction-site inspection view, textured
skeletal Tommy). Audio and interactive input on Windows remain unverified.
`cargo +1.99.1 test --workspace --locked` runs the Rust tests, but does not
replace all checks in `scripts/test.sh`.

## macOS — build and Metal rendering tested; audio and interaction not yet

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

### macOS Metal runs (Apple Silicon, verified 2026-11-03)

Native build and Metal rendering run on macOS; audio and interactive input are
not yet verified. The winit AppKit backend compiles automatically (no manifest
change), so windowed commands open native windows:

```sh
./scripts/run.sh --root /path/to/Mafia --model models/TommyTAXIDRIVER.4ds --animation anims/breath01a.5ds --skeletal-animation --headless --animation-steps 100 --capture /tmp/tommy.png
./scripts/run.sh --root /path/to/Mafia --mission mise01 --headless --capture /tmp/mise01.png
./scripts/run.sh --root /path/to/Mafia --mission mise01 --frames 120 --capture /tmp/mise01-win.png
```

The skeletal capture renders a textured animated model; the MISE01 capture
renders the mission scene (bridge, construction site, city blocks; inspection
view, no HUD). The bounded windowed run presents native frames and captures
bit-identically to the headless run. Interactive driving, menu input and audio
on macOS remain unverified.

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

Diagnostics also shows a bounded 120-redraw history with p50, p95, maximum
intervals and counts above the 60/30 Hz frame budgets. The bars report actual
host redraw spacing (including stalls), not GPU timestamps or simulation time;
focus changes and scene reloads reset the history.

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
