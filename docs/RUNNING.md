# Build and run

The program reads a legally obtained original PC installation through `--root`.
Keep game files outside this repository. Rust 1.99.1 and `Cargo.lock` are pinned.
The original executable and reverse-engineering tools are not runtime inputs.
The menu and mission commands inspect current resources; the first mission is not
playable from start to finish.

## Installation asset check

```sh
cargo run --locked -p lhr-assets --example inspect -- --check-installation /path/to/Mafia \
    'Missions\freeride\clusters.seg' 'Missions\freeride\scene2.bin'
```

The inspector reads each requested relative path through the installation
reader. Output gives the argument's ordinal and one of `readable`, `missing`
or `read-error`. A successful read also reports the byte count. A refused
path or a failed read of the selected archive or loose source is a read error.
The inspector checks readability without parsing the payload format,
extracting data, or starting the game.
Exit codes are `0` when all entries are readable, `1` for any missing entry
or read error, and `2` for missing arguments or installation setup failure.

## Original Freeride startup diagnostic

```sh
./scripts/run.sh --freeride-start-probe --root /path/to/Mafia --probe-model-dir Models
```

By default this diagnostic is headless and fixes the original mission name to `freeride`. It
excludes `--mission`, campaign probes, standalone car-driving, model-selection
and unrelated presentation flags; optional `--probe-render-capture FILE.png`
attempts the existing GPU presentation at common Init's reached Scene render.
It supplies no substitute map, default car or implicit
model directory. It exits unsuccessfully while startup is blocked.

The fresh-session sequence is ClosePrevious, original source Open, collision,
mission-number reset, SaveGameNewGame, FreerideSetup, Init, FreerideSetupAfter,
GameLoopSingle and Close. Only the reached prefix executes. After a complete
Open, synthetic tests and a headless run on the original installation reach
actual PLAYER/CAR construction and actual model roots named `Tommy` and
`TomyhoAuto`. Startup reads `tables/carindex.def` and `tables/playerindex.def`
through the same loose/DTA installation owner. Without a selection it stops
before GetCarModel: the actual GM_FreeRide menu is not reconstructed, and neither
the profile nor an invented default supplies a car/location choice. Init and all later
steps remain unentered at that stop. Source Open failures stop earlier. The
original-data run completes collision loading and terminal disposal with zero
remaining represented owners; its nonzero exit still denotes blocked startup.

For dependency diagnostics only, `--freeride-selection
CAR,FLAGS,LOCATION,TRAFFIC,PEDESTRIANS,POLICE` supplies all six inputs explicitly.
The first three are unsigned decimal words (CAR is the original database
descriptor, not a model filename); densities are parsed as f32 without clamping.
The input writes the real StoreA/StoreB state before Open and survives
SaveGameNewGame, which only clears active. Setup reads car at GetCarModel,
location after car cache Open, and flags only after car Init. The later script
command `FREERIDE_TRAFFSETUP` applies the densities only for profiles with a
Freeride unlock value of at least 3; the fresh profile does not take that branch.
Invalid car indices stop instead of using the native caller's unwritten name
buffer or inventing a fallback. Scene lookup searches primary-sector children
by original name with the native `0xffff` mask, then tries literal `emeth_1` only
when no requested marker exists. Missing both markers skips placement, not Setup.
Found markers supply position/direction separately for the car and player, then
the real model owners perform Update, AddFrame and creator Release in native
order. Car Init, optional roof-off, Tommy cache Open, player Init and AddActor
follow. Native false Init returns and failure HRESULTs are recorded but ignored;
host errors retain their reached prefix and stop. Roof-off (`flags & 0x100`) has
no reconstructed owner and stops explicitly. Synthetic fixtures and the
original-data run below complete Setup without that flag and enter common Init,
which stops at the unavailable Scene render in the default headless
ResetMissionState path, not gameplay.
On original assets, selection `1,0,1,1,1,1` resolves `fordttud00.i3d` and `emeth_1`,
places both models and transfers them to the Scene, opens Tommy, initializes
both actors successfully and adds them in native order. The prior Car Init
high-byte-name failure is resolved by the evidenced LS3DF initial-C-locale
comparison, independent of the Game CRT. Names are not decoded, renamed or
ignored; embedded NUL and unreconstructed locale-dependent comparisons still
stop. Terminal disposal succeeds with zero represented mission owners.
Source/numeric/cache errors can stop earlier. This option is not native menu execution,
an interactive mode, or Freeride acceptance.

With `--probe-render-capture /tmp/freeride-init.png`, the same one-shot startup
attempts a GPU image of the retained Scene only when Init reaches its render.
Failure before that call produces no image; a capture failure retains the muted
prefix and stops. The later loading-menu Draw/Present has no composed owner and
is explicitly refused, not treated as a successful presentation. A captured
frame is not a successful game tick or playable control.
The original-data capture with selection `1,0,1,1,1,1` now succeeds after the
source-backed spot-light selection/cone branch; Init stops at PlayerNavi's
loading-screen presentation. Terminal disposal again releases all represented
mission owners, including the prepared visual buffers. Spot lighting retains
the existing checked PC24 numeric domain and receives no whole-function credit.

With `--probe-window-pc24-layout SCALE_X,SCALE_Y,LEFT_MARGIN`, the diagnostic
instead opens a window and uses one swapchain for actual source Open loading
frames, the retained Init Scene, and reached Init loading frames. It requires
a surface-compatible GPU and a non-sRGB target supported by that surface.
Supply positive finite scales and a finite left margin explicitly; PC24
round-to-nearest is a diagnostic condition, not evidence of the original
incoming control word or layout. It cannot be combined with
`--probe-render-capture`. The window hosts one attempt, not an interactive
menu or game loop: it installs no active cursor, and Open's camera-less 3D
Render and global blend step remain diagnostic. The report counts scheduled
presents, not observed frames. A reached error stops startup without replay.
On Windows with Direct3D 12, the original-data selection above scheduled 25
Open loading frames, the Init scene and one Init loading frame at 800×600
with layout `1,1,0`. On November 23, 2026, the first run completed
`GameInitStart`, including `TraffSndSect` and `FREERIDE_TRAFFSETUP` (389),
then stopped at `SegmentsLoad` with `original asset unavailable`.

A catalog check found `scene2.bin` for Freeride and `clusters.seg` for other
missions, but no `Missions\freeride\clusters.seg` in the 13 registered archives
present in this installation or in the loose mission directory. A second DX12
run with the same inputs reported `MissingOriginalAsset` for that exact path
at open. Errors reading a selected source remain distinct from absence. The
second run scheduled the same frames and completed terminal disposal; it did
not reach player/car coupling or a gameplay tick.

The original loader reads the header without checking whether open or read
succeeded. In an isolated Wine 8.0 and D8VK run, the original PC executable
failed to open `Missions\freeride\clusters.seg`, returned zero to Init, and
continued into playable Day City with Little Italy as the starting district.
Accelerating moved the Falconer Yellowcar from the parking area toward the
street. The debugger set the city unlock flag in the test profile's running
process. In a repeat run with the same isolated profile, `dtaOpen` returned
`-1` and the first `dtaRead` returned `0xffffffff` for the 16-byte header
request. The loader again returned zero to Init and Day City loaded. The Rust
diagnostic still stops at the missing file.

When the segment loader returns normally (including a rejected header), Init
clears the new-mission flag and checks `intern_load`. If set, it calls
`LoadTyres` on the car actors' shared vehicle tables. A missing tyre file
returns false and leaves only the built-in user tyre; a short read interrupts
the load. Either normal tyre return reaches `LoadMaterials`. The Freeride
diagnostic reads the startup definitions (items, materials, particles, voices)
after the indicator asset prefix and before opening the mission. A missing
file ends that intake normally, retaining earlier tables. A read error stops
startup instead. The installation supplies complete file images, not native
short-read timing. A car constructed before mission Init can still use the
existing lazy table reader. At Init, the retained material records replace
those rows without reopening the file. If intake never returned, Init refuses to replace
the materials; if it returned with zero material records, the original loader
publishes its single default row. The next stop is `LoadParticles`, where
vehicle emitter creation and cleanup are not connected. With `intern_load`
clear, Init skips all three loaders. These paths are exercised with synthetic
segments; they do not move the original-data run past its missing segment file.

`SETTRAFFSECTORSND` (317) registers a matching primary scene sector for vehicle
sounds; it does not start audio or traffic. With the fresh profile used in this
run, opcode 389 copies only the race-car mask to the stealable-car mask. When
the profile's unsigned Freeride unlock value is at least 3, it first copies the
selected traffic, police and pedestrian densities to the car and pedestrian
traffic settings. That branch is covered by synthetic tests, not by this run.
Existing profile files are not decoded or silently treated as fresh profiles.

The script compiler rejects non-ASCII bytes in quoted sector names before
`SETTRAFFSECTORSND` runs, even if the scene contains a matching name.

Native application database state is session-owned here and retained through
terminal mission disposal. Cross-session application ownership is not composed.
Its startup intake does not emulate AllDefault, repeated native database Init,
native allocation failures or short reads; it preserves completed tables and
refuses replay after a host failure. Lookup uses the evidenced low-16-bit index,
top-byte variant and `%s%02d.i3d`, including the optional +10 second-model variant.

The existing LoadSaveCarDatabase interface for race/garage lookups still has only
test implementations; the startup owner is not connected to those gates yet.

The report includes reached steps and retained owners, then explicitly disposes
the same content and model owners. Disposal is terminal host cleanup, not the
native active-mission Close or a repeatable gameplay lifecycle. Setup cannot
be replayed after an interruption. Constructor seeds, device-free input and
headless presentation remain disclosed host policies. No successful game ticks,
interactive Freeride, player/car coupling, traffic or runtime parity is claimed.

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
command. `--car-driving` cannot be combined with `--headless`.

Driving keyboard presses are latched until the next actual 16 ms simulation
step, so a press/release between redraws is not lost. A released tap applies
to one catch-up step; held input stays continuous. Focus loss, UI capture,
pause and reload clear pending input. This is host input behavior, with no
original-input parity or controller/aim-assist claim.

The optional Engine Lab needs the `engine-lab` feature and a window;
F1 toggles its panel, F2 pauses,
F3 steps and F4 reloads. It is disabled in the default build. The CPU
`--mission-load-probe` runs before the window/audio device path and does not
execute gameplay. Pass `--probe-model-dir Models` explicitly for model intake;
its intentional default is an empty directory list, so a run without that
argument proves no model geometry.

For a fresh installation with no numbered profile save, the probe now applies
the original 60-row control defaults to a real control/keyboard state in an
explicit keyboard/mouse-only host mode. Joystick bindings remain in the profile
but are rejected by the device-free runtime, as `SetControl` requires. No
joystick discovery or platform input polling is claimed. Existing profiles
are refused until their G_Stream decoder is available, never replaced with
defaults. With the earlier dependencies satisfied, Init can complete
force-feedback capability initialization and `blood_manager.GameInit`.
Neither step starts gameplay. `DiffClear` empties the actual
borrowed frame/actor lists and resets the pending-diff flag and crash-object
counter without deleting the referenced objects. `LoadDiffData`, runtime crash
actor updates and nonempty `ClearDiffData` teardown remain uncomposed.

The live mission owner now binds the `C_mission::Open` mute/restore effects
and the first `Tick` restore-volume gate to the shared Rust PCM master gain.
`skip_frames` delays that one-shot Tick effect; an interrupted audio write
retains the already-consumed flag. This is bounded host audio behavior, not a
complete mission Tick, native DirectSound volume curve or playable GameInit.

`MissionLoadSession::open_source_in_order` loads actual world, weather,
sector, navigation, road and scene2 owners in the represented Open order on
a fresh session. It requires a caller renderer between audio mute and the
scene2 read. Missing scene2 restores audio without creating a map/camera;
present corrupt or unsupported content retains its reached loader error.
Scripts and Init stay blocked until the entire sequence, including the final
cache read, succeeds. It shares loading helpers with the existing probe;
Game reset, vehicle/progress/media markers, native active Close and playable
startup remain outside this source-loading path.

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
The script diagnostic uses the session's represented game fields for
`game_nightmission` and `noanimpreload`. It now refuses unowned light-cache
and city-traffic effects at their reached instruction instead of counting
them as successful. A previously successful diagnostic can therefore stop
earlier; its program counter and executed state remain available for review.
For explicit host calls, `MissionLoadSession` also exposes
`initialize_owned_entity_programs`, `initialize_owned_detector_programs`,
`tick_owned_entity_programs` and `tick_owned_detector_programs`. They use the
retained actors, Scene and game fields, the session precision scope and the
caller's shared dispatch budget. A host dispatch lease restores the same
embedded program on errors and panic; interrupted state stays blocked against
replay. Diagnostics may mutate state and retire other actors, but current-owner
lifetime/source changes refuse before effects. Replacing the entire borrowed
`ActorWorld` violates the callback contract and loses restoration; it cannot
attach the old program to a new world or mask the original panic. These calls
do not establish GameInit or a native tick order and are not added to the probe.
Terminal source disposal also checks embedded actor programs before entering
any destructor. After an explicitly stopped runtime, dispose its Rust VM
storage through `discard_actor_program_runtimes_host` (and the existing
global-program counterpart) before terminal source cleanup. A detached
program must first return to its actor. This is host cleanup for a fresh
session, not original GameDone or proof of a playable mission restart.
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
from input and disposal failures. The capture now runs at the reached render
boundary of the composed new-game Init (after `SaveGameNewGame`) when
the start reaches it, otherwise of the independent ordered Init inspection,
after the live audio mute. A failed
or unreached presentation is not retried outside that order. Successful
presentation advances through the remaining ordered Init dependencies;
the fresh-profile keyboard/mouse path described above reaches `BloodAndFramePools`,
not completed GameInit. Store captures outside the source tree;
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
command yet. Supported commands, original comparisons and gaps.

The explicit skeletal command is an existing host inspection path, separate
from the new model-owned controller and mission actors. Its 100 controlled
steps sample 1,600 ms; this command and the zero-step baseline were rendered
successfully at `b3b6b67`, changing 18,563 pixels. Omit `--headless`,
`--animation-steps` and `--capture` to open the animated model in a window;
this batch verified the headless command, not the window controls.

## Windows — native build and run

Install the official Rust 1.99.1 MSVC toolchain, Visual Studio Build Tools with
the C++ workload and Windows SDK, and a graphics driver for Vulkan or Direct3D 12. Use
PowerShell in the repository root. The Bash setup scripts are not native Windows
commands. Windows uses Vulkan by default. To run the same renderer and
Freeride startup window through Direct3D 12, set `$env:WGPU_BACKEND = "dx12"`
in PowerShell before launching. Remove the variable to restore the default.

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

### Full test suite on Windows (Git Bash + WSL)

Run `scripts/test.sh` from Git Bash. The fmt/check/test/clippy steps use the
native MSVC toolchain from `scripts/common.sh`. The Python suites test Linux
behaviour (`/proc`, `prctl`, process groups, POSIX modes and symlinks), so they
always run in WSL. No native Windows Python fallback exists. The WSL,
distribution, tool and line-ending checks run before the cargo steps and stop
the run with an error if anything is missing.

- The distribution is selected explicitly, because the WSL default may be
  `docker-desktop`. The default is `Debian`; override it with
  `LHR_WSL_DISTRO=<name> scripts/test.sh`. `wsl.exe -l -v` lists the installed
  distributions.
- The distribution must provide Bash, Python 3.11+, `git`, `tar`, `grep`,
  findutils (`find`) and coreutils (`dirname`, `uname`, `env`, `mktemp`,
  `stat`, `rm`, `mkdir`, `sort`, `cp`, `tr`). On Debian, install them with
  `sudo apt install bash python3 git tar grep findutils coreutils`.
  Analysis installation tools such as `curl`, `unzip`, Node and npm are not
  required by the tests: setup validates the private path before checking
  installer prerequisites. Its symlink-refusal test uses an isolated PATH
  containing only the tools needed for that validation. The tests download nothing.
- WSL copies the working tree into a private `0700` directory on the Linux
  filesystem and runs the tests there. The copy includes tracked and
  untracked, non-ignored files, and their bytes are unchanged. The directory
  is removed afterwards. On the `/mnt/<drive>` mount every file reports mode
  `0777`, and path resolution is slow enough that `test_rewrite_progress.py`
  did not finish within 10 minutes. Inside the copy, `core.autocrlf` is
  disabled for the tests' temporary repositories only.
- `.gitattributes` keeps `*.sh` at LF. The runner reports any `scripts/*.sh`
  that still has CRLF, for example from a checkout made before the attribute
  existed. Re-checkout each unmodified file with
  `rm scripts/<name>.sh && git checkout -- scripts/<name>.sh`.

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

The last command requires a working graphics adapter (Vulkan, Metal, or
Direct3D 12 on Windows). Native macOS/Metal
and original installation interaction need separate runs; synthetic checks do not
establish either result.
