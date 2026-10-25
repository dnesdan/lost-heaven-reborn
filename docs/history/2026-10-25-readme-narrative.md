> Historical narrative from `README.md` at baseline `0da90fb`, before the texture/Scene/actor-loader publication. Original narrative wording is retained below; only relative link destinations are rebased for this archive location. Current status is in [README](../../README.md), [coverage](../../COVERAGE.md), [progress](../PROGRESS.md) and [goals](../GOALS.md).

# Lost Heaven Reborn



Static percentages retain raw analyzer denominators, including external import rows. The audited in-image cohorts now have output for 4,784/4,784 Game, 1,555/1,555 LS3DF and 176/176 rw_data entries. These are recognized cohorts, not exhaustive function discovery or completed Rust rewrites; 491 pinned mafia-re LS3DF addresses still need exact inventory correspondence. Scoped ports, warnings and runtime validation remain separate, with history preserved.

An independent Rust reimplementation of the Mafia: The City of Lost Heaven (2002) engine and game. The goal is an engine that runs the original game's models, animations and campaign from a user's legally obtained PC installation. Current functionality includes the Czech PC menu, bounded scene/model loading, an owned frame/Joint/Target hierarchy, and sequential skeletal animation through recovered controller, clock and dispatch paths. The campaign is not playable. No runtime game data is included.

The animation path binds original track names, advances one live controller slot, evaluates position and bounded linear/spherical rotation, updates Target and morph state, and uploads CPU-skinned first-LOD geometry. Controller ownership and controls cover eight slots; blended evaluation, scale/note channels, full animation-resource ownership and actor/mission integration remain open. Unsupported inputs fail explicitly. Original models and clips are regression fixtures for this general implementation.

Standalone object/mesh/material ownership includes bounded managed-record creation, Request queue production, pool unload/release/restore and texture/string cleanup. Bounded standalone managed texture Restore now reconstructs admitted system/default pools and reserves the registry through controlled device/storage effects. It composes the accepted Release dependency, selects raw format once, uses returned descriptor Size with signed budgets and wrapping memory arithmetic, grows arrays in 1,024-slot steps and preserves registry entries. RestoreManagedTextures is a pinned mafia-re working name without recovered original PC/PS2 symbol. Admitted plain-object loaded resources now connect to CPU ModelArena ownership. Full native texture Create/worker/upload, real GPU/device implementation, whole-device shutdown, derived lit/skin/morph resources and application/mission integration remain open; these standalone resource contracts establish no original-renderer parity.

Ownership code uses `lhr_engine::ls3df::{c_str, itexture, i3d_object, texture_manager}`. These logical original-library modules add no DLL or Cargo package; see the [package and name mapping](../REFERENCE_PROJECTS.md#rust-organization-and-original-names).

Scene dependencies now include recovered weather reset and sector sound defaults,
compared against original instructions in 800 inputs including precision controls.
Standalone resource cleanup follows vertex → index order with shared-owner and
failure-prefix checks. A fresh driving regression exercises three reloads and four
zero-owned-resource cleanups; [view its screenshot](../screenshots/2026-10-25-resource-reload.webp).
Full Scene construction,
shared Snow ownership and mission integration remain open; see [progress](../PROGRESS.md).

The immediate goal is **the playable first mission, from the original menu through mission completion**, alongside function-by-function library parity with mafia-re. See the [acceptance gates and dependency order](../GOALS.md). PS2 symbol names guide reconstruction; PC behavior and platform differences require original-binary verification.

The [fresh mafia-re, MafiaUnity and OpenMF-Archived audit](../REFERENCE_PROJECTS.md#current-source-audit--2026-10-25) identifies the next dependency: real file-backed texture/cache ownership for Scene construction, then shared weather lifetime and original mission ownership. This source inspection adds no implementation or coverage credit.

Standalone original-data driving now composes Car construction, Init and
GameInit with the recovered Tick/Engine/Move/Update workers, original world
collision and vehicle material/tyre tables. One live owner supplies the car,
wheels, lights, skid models, sound frames and explicit GameDone/destruction.
Nearby original cache-model draw subsets refresh as the car moves. Generic
`C_crash_obj` creation and physical debris share the canonical dynamic collision
context. Original WAV engine and collision sounds feed the mixer, optional CPAL
output and WAV capture; runtime requires assets without opening an original
executable or DLL.

Use `--car-driving`: W/S accelerate and brake, A/D request full steering,
Space applies the handbrake, and R reloads the standalone context. This mode
uses the authored MISE01 car placement and a host chase camera. Mission scripts,
traffic, occupants and original presentation effects remain separate work.
Rendering is unlit first LOD; remaining particle, projection, actor-class and
resource paths fail explicitly. That driving integration added no new canonical
entries, whole-function or campaign credit.

The frozen Source4 passes the full check suite and three headless trajectories
in both actual debug/release profiles: 900 collision steps, 900 glass-damage
steps with camera/listener/PCM updates, and 1800 idle steps. Owned resources
return to zero; five caller-owned Scene-prefix frames remain separate context.
Windowed tests in both profiles verify acceleration, full A/D steering requests,
S braking, Space, focus loss and R reload. Damaged glass material is observed;
exhausted-window fragments and their sound remain unexercised. All admitted
original-instruction comparisons pass in actual Cargo opt0/opt3 builds, retaining
controlled world/resource interfaces and separate CPU/projection scopes.
Original code runs under masked CW007f PC24/RN; normalized Rust results/effects
and its modeled logical sticky P are compared. Rust/native host hardware
CW/MXCSR equivalence is unclaimed.
Both sequential independent non-author reviews approved the bounded Source4
milestone with no open findings. The accepted source is commit `78c130e`.

[![Original taxi during the continuous driving capture](../screenshots/2026-10-24-car-driving-verified.webp)](../videos/2026-10-24-car-driving.mp4)

[Watch the 18-second release driving video](../videos/2026-10-24-car-driving.mp4)
· 640 × 426 · 55.038 metres maximum displacement. It includes 17.44 seconds of
original-resource stereo audio at 48 kHz. Software capture and the capped
simulation clock diverge by up to 0.706 seconds; audio was neither stretched nor
padded. [Capture scope and limits](../screenshots/README.md#2026-10-24--continuous-driving-preview).

The car inspection mode now previews recovered steering on the same initialized wheel frames. Hold A or D to turn the wheels; arrows and W/S control the camera, and R explicitly reloads the Car and resets input. The car body stays stationary.

Actual authored MISE01 remains FINDACTOR/UnsupportedWorldBackend at PC 3 / source line 4; no later line executes. All six first-mission gates remain open, with strict verification 0/12.

| Taxi steering left · 960 × 640 · 47.5 KiB | Taxi steering right · 960 × 640 · 50.2 KiB | Police steering · 960 × 640 · 48.8 KiB |
| --- | --- | --- |
| ![Original taxi with recovered left wheel steering](../screenshots/2026-10-24-car-steering-taxi-left.webp) | ![Original taxi with recovered right wheel steering](../screenshots/2026-10-24-car-steering-taxi-right.webp) | ![Original police car with recovered wheel steering](../screenshots/2026-10-24-car-steering-police.webp) |

Actual release-build Linux software Vulkan captures, dated 2026-10-24. Lossless WebP exports retain every captured pixel. Rendering is unlit first LOD; the images demonstrate wheel steering without body motion or playable driving. [Reproduction and limits](../screenshots/README.md#2026-10-24--recovered-car-wheel-steering).

| Czech PC menu in Rust | MISE01 static inspection in Rust |
| --- | --- |
| ![Rust engine showing the Czech menu](../screenshots/2026-10-21-menu.webp) | ![Rust engine showing the static MISE01 neighborhood and taxi](../screenshots/2026-10-21-mise01.webp) |

[Dated screenshot history](../screenshots/README.md) · 176 KiB total for these two previews. The mission view is not playable.

![Original civil02 character in a static pose through the recovered skin pipeline](../screenshots/2026-10-22-original-skin.webp)

Actual Rust capture using original geometry and textures; first LOD, static pose and inspection lighting. [Evidence and reproduction](../screenshots/README.md#2026-10-22--original-skinned-character).

![Original Tommy at a sampled animation pose in the Rust engine](../screenshots/2026-10-22-tommy-skeletal.webp)

Actual 1,000 ms pose sample, original geometry and textures, inspected through X11. Preview: 640 × 427, 2.9 KiB. [Reproduction and limits](../screenshots/README.md#2026-10-22--original-tommy-skeletal-pose).

## Build and run

Requires Rust 1.99.1, installed by the setup script. Linux uses wgpu with Vulkan and requires `pkg-config` plus ALSA development files (`libasound2-dev` on Debian/Ubuntu) for native audio. macOS is intended to use Metal and requires an Apple SDK for CoreAudio bindings. Dependencies are pinned in `Cargo.lock`.

```sh
./scripts/setup.sh
./scripts/test.sh
./scripts/run.sh --synthetic
```

Build the native engine binary with `cargo build --locked -p lhr-engine` from the repository root. These commands use the user's game installation directly:

```sh
./scripts/run.sh --root /path/to/Mafia
./scripts/run.sh --root /path/to/Mafia --model models/taxi00.4ds --car-inspection
./scripts/run.sh --root /path/to/Mafia --model models/taxi00.4ds --car-driving
./scripts/run.sh --root /path/to/Mafia --model models/taxi00.4ds --car-inspection --steer 1 --steer-steps 120
./scripts/run.sh --root /path/to/Mafia --mission mise01
./scripts/run.sh --root /path/to/Mafia --model models/4build81.4ds
./scripts/run.sh --root /path/to/Mafia --model models/civil02.4ds
./scripts/run.sh --root /path/to/Mafia --model models/TommyTAXIDRIVER.4ds --animation anims/breath01a.5ds --skeletal-animation
./scripts/run.sh --root /path/to/Mafia --model models/TommyTAXIDRIVER.4ds --animation anims/breath01a.5ds --sample-animation-ms 1000
./scripts/run.sh --root /path/to/Mafia --model models/sipka1.4ds --animation models/sipka1.5ds
```

The first command opens the Rust-rendered original menu. Up/Down and Enter select options; only New Game and Exit work, and there is no profile chooser. New Game switches to a static intro scene; Escape switches to the bounded MISE01 spawn-neighborhood inspection. This temporary inspection path does not play the original video/cinematic or execute missions or gameplay. Add `--headless --capture /tmp/scene.png` to either inspection command for a one-frame GPU capture, or `--frames N` to bound a window run. Camera inspection uses arrow keys and W/S. See [coverage](../../COVERAGE.md) for exact evidence and limits.

`--steer` accepts a finite request from -1 to 1. `--steer-steps` accepts 0 through 600 and defaults to 1; each step uses a controlled 16 ms host inspection clock. With no steering request, the existing static inspection route is unchanged.

Standalone static `--model` loading now builds an owned mesh/dummy hierarchy using recovered common LS3DF fields, parent links, reference transitions and guarded original transforms. `4build81.4ds` exercises three meshes and one dummy. Unsupported frame types, world-space visuals, reflective materials and unverified numeric paths report errors. This route does not establish native world streaming or original lighting/material parity. The MISE01 neighborhood remains a separate static inspection path.

Standalone models use the owned original hierarchy and checked first-LOD skin/morph pose. Earlier skin intake admits 80 of 82 selected paths (37,948 vertices); two remain explicit numeric-domain failures. Separate composed comparisons cover `civil02` and the actual `TommyTAXIDRIVER` model. Admitted loaded plain-object visuals now use recovered CPU ownership; original GPU upload integration, full native texture Create/worker/upload, whole-device shutdown and original lighting remain open. The inspection renderer does not use recovered normals for lighting.

`--skeletal-animation` runs sequential position/rotation playback for a supported model/animation pair. It explicitly requests slot 0, unit weight and loop mode; original game animation selection is not implemented. Windowed playback uses the recovered clock and dispatch path. Add `--headless --animation-steps 25 --capture /tmp/animation.png` for 25 controlled 16 ms steps. Bounds, deformation and rendering are requested by the inspection adapter; this does not establish original renderer scheduling. Spherical rotation admits angles through 1.5 radians only where checked numeric intervals give an unambiguous original float result. Other domains, multi-slot blending, scale and notes fail explicitly.

`--sample-animation-ms` retains independent position/rotation pose evaluation at the supplied time, followed by Target updates and skinning. Original `breath01a` samples at 0, 40, 200, 1000 and 5240 ms remain regression evidence. This option excludes sequential playback and `--animation-steps`.

The `sipka1` command without a sample time retains the earlier single-root position playback path through the recovered clock, model and interpolator. Add `--headless --animation-steps 25 --capture /tmp/animation.png` for 25 controlled 16 ms steps. Captures at 0/25/50 steps verify movement and exact return; inspection camera, handedness, materials and rasterization remain separate from original presentation.

Current Source4 checks pass 1066 Rust tests (110 assets unit, 7 assets integration,
910 engine library, 8 engine integration, 26 application, 5 renderer), nine
REA-wrapper tests and 47 progress tests, plus formatting, locked checking and
all-target Clippy. The 264-file source guard agrees before and after the gate.
Headless trajectories, legacy pixels, windowed driving controls and the release
video have separate source-bound receipts. The scoped native closure passes in
both Cargo profiles, and both sequential independent reviews are approved.
Original renderer parity and complete-system acceptance stay separate.

Linux software Vulkan uses the same initialized CPU arena for the car, lights,
shadow and skid models. Compatible draws receive vertex updates; visibility or
material changes replace only the affected GPU draw range. Default taxi/police
and static MISE01 regression pixels remain unchanged. Rendering is unlit first
LOD with nearest sampling and the supported host 0.5 mask cutoff; original
lighting, reflections, blending and native GPU parity remain unverified.

The next goal is integration with original Game/player/mission ownership and
scheduling, complete Scene construction/Close and remaining live resource
factories. Standalone flags 2/audio and Car GameInit are bound; authored mission
execution remains at FINDACTOR/UnsupportedWorldBackend.

`setup.sh --graphics` prepares the Linux software Vulkan tools used for development captures. `scripts/test.sh` checks formatting, compilation, tests, Clippy, REA wrappers and generated progress. On Linux x86_64, X11 input has been checked through menu, static INTRO and the MISE01 neighborhood, including orbit and reload. The current Apple arm64 check reaches the new CoreAudio dependency and is blocked in this Linux environment by missing `AudioUnit/AudioUnit.h`; the earlier source's successful check remains historical. macOS linking and Metal runtime have not been tested. iOS is a future compatibility target. The original game's executable and REA are not gameplay runtime dependencies.

## Reverse engineering and assets

REA with Ghidra is the required analysis path for engine behavior. Evidenced original function/type names are preserved, with Rust naming conventions; descriptive working names remain explicitly distinct from recovered symbols. Keep original executables, installations, extracted or converted assets, analysis databases, decompiler output, credentials and private URLs outside Git. Public tests use synthetic fixtures. The reader supports selected PC DTA archives, bounded model/scene/menu/script formats and PCM16 mono/stereo WAV archive audio. Other audio codecs and archive checksum verification remain open. Full city simulation, mission execution and campaign play are not implemented. Use a trusted, stable installation tree. PS2 material is used for research and comparison.

Project code is MIT licensed. Game assets and trademarks belong to their respective owners. See [third-party notices](../../THIRD_PARTY_NOTICES.md), the [mafia-re/MafiaUnity reference audit and PS2 symbol crosscheck](../REFERENCE_PROJECTS.md), and the [asset inventory and optional graphics assessment](../ASSET_ENHANCEMENT.md).
