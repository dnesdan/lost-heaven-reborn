# Lost Heaven Reborn

Godot 4.6.3 development prototype for a future Mafia (2002) remake. The current
slice uses original procedural geometry: walking, a third-person camera, a car
with enter/exit and prototype driving, checkpoints, failure, completion, and
restart. It contains no Mafia models, animation, text, or audio. Vehicle handling
is an approximation; fidelity to the original game has not been established.

Local validation: 24 synthetic Python tests and 25 headless Godot assertions pass;
the Linux export starts independently with no script errors. The actual district,
player, car, checkpoint and HUD were inspected in a Mobile/Vulkan llvmpipe capture.
Private evidence stays outside the checkout. Public CI has not been executed yet.

Linux uses Mobile/Vulkan. A verified software Vulkan renderer is suitable for
development captures; Xvfb itself supplies no GPU acceleration. Headless tests
prove logic, while rendered captures and real input checks must be reported
separately. macOS and iOS presets are preparation only: Metal, signing, touch,
pause/resume, and device behavior require Apple hardware and Xcode validation.

## Commands

Run from the checkout on Linux with Python 3, curl, unzip, and sha512sum:

```sh
./scripts/setup.sh                  # official pinned engine + matching templates
./scripts/setup.sh --graphics       # also unpack verified graphics tools on Debian 13 x86_64
./scripts/run.sh                    # requires a working Vulkan display
./scripts/test.sh                   # public synthetic Python + headless Godot tests
./scripts/capture.sh                # private isolated X11/Vulkan screenshot
./scripts/export.sh Linux           # build/linux/lost-heaven-reborn.x86_64
./build/linux/lost-heaven-reborn.x86_64
```

The exported game runs independently of the editor and analysis tools on a host
with writable user data and a Vulkan driver. In this restricted cloud shell,
`source scripts/common.sh` before starting the exported binary supplies writable
XDG locations. The public CI workflow uses only synthetic tests/resources;
workflow configuration is not evidence that GitHub Actions has executed it.

Capture additionally needs Xvfb, xdotool and ImageMagick (`import`); video recording
can use ffmpeg. On Debian/Ubuntu the system
packages are `xvfb xdotool imagemagick ffmpeg libvulkan1 mesa-vulkan-drivers`.
Use a hardware Vulkan driver when available. `GODOT` may select another executable
of the pinned version. Tools/cache/config/data live outside the checkout under
`/workspace/lhr-tools` and `/workspace/.{cache,config,local}` by default; existing
`XDG_*`, `LHR_TOOLS_DIR`, and `LHR_PRIVATE_DIR` overrides are respected.
The Debian helper verifies signed APT indexes and fixed package checksums without
installing host packages. Its wrappers select software Vulkan when `/dev/dri` is
absent; `LHR_SOFTWARE_VULKAN=1` explicitly selects it and `=0` leaves the host ICD.
Distribute the generated `GODOT_NOTICES.txt` and `LICENSE.txt` with exported games.

WASD walks/steers; mouse moves the camera; E enters/exits the nearby car; Space
brakes; R restarts; Escape pauses. The prototype provides shared input actions
and gamepad/touch controls; real controller and Apple-device behavior still needs
hardware testing. Export presets are named `Linux`, `macOS`, and `iOS`.
`./scripts/export.sh macOS` produces the preset artifact, while iOS export/signing
must finish on a Mac with Xcode. These commands do not establish Apple readiness.

## Private inputs and intermediate data

Provide a lawful local file or configure a direct download URL securely using
`MAFIA_PC_URL`, `MAFIA_PS2_URL`, or optional `PS2_BIOS_URL`. Do not put source URLs
in commands, documentation, issues, or commits. Website landing pages need a
separate authorized download step; intake does not resolve file-host pages.

```sh
./scripts/import.sh intake --kind pc --edition 'PC Czech edition' --local /private/input.zip
./scripts/import.sh intake --kind ps2 --edition 'PS2 USA NTSC'
./scripts/import.sh validate-level /private/level.json --output /private/validated.json
```

Intake preserves a read-only private copy, verifies SHA-256, and records the
user-supplied edition label without storing source URLs or filenames in manifests.
Downloads resume when a server supplies a strong ETag and valid range responses;
otherwise they restart safely. Original local files are never modified. Files
are not extracted or executed. Private storage is rejected inside the checkout.
The manifests identify bytes, not authenticated game editions.

`lhr-level-v1` validates separate scene/collision metadata in meters and a
right-handed Y-up coordinate system. Objects have unique IDs, finite positions,
optional rotation/positive scale, relative GLB model paths, box collision, and
JSON metadata. Public tests generate synthetic fixtures. This validator does not
convert DTA/4DS/5DS, validate GLB skeletons/materials, or load original assets into
the game. No proprietary-format converter has been implemented or validated.

Next: identify the provided PC edition, extract one model with a compatible
licensed tool, validate axes/scale/materials and skeleton/animation timing against
private reference captures, then integrate an original district and car. Install
Wine/emulator/REA/Ghidra only for a specific validated reference-analysis need.
PC and PS2 reference runs, behavior comparisons, original-asset milestones, and
Apple hardware checks remain outstanding until their prerequisites are available.

Our original code is MIT licensed; game assets and trademarks remain with their
owners. See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for inspected references.
