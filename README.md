# Lost Heaven Reborn

Godot 4.6.3 development prototype for a future Mafia (2002) remake. The current
slice uses original procedural geometry: walking, a third-person camera, a car
with enter/exit and prototype driving, checkpoints, failure, completion, and
restart. It contains no Mafia models, animation, text, or audio. Vehicle handling
is an approximation; fidelity to the original game has not been established.

Local validation: 24 Python tests, 6 synthetic model/converter tests (including
a headless GLB load), and 25 gameplay Godot assertions pass;
the Linux export starts independently with no script errors. The actual district,
player, car, checkpoint and HUD were inspected in a Mobile/Vulkan llvmpipe capture.
Private evidence stays outside the checkout. Verification uses local synthetic tests.

Linux uses Mobile/Vulkan. A verified software Vulkan renderer is suitable for
development captures; Xvfb itself supplies no GPU acceleration. Headless tests
prove logic, while rendered captures and real input checks must be reported
separately. macOS and iOS presets are preparation only: Metal, signing, touch,
pause/resume, and device behavior require Apple hardware and Xcode validation.

## Commands

Run from the checkout on Linux with Python 3, Node.js 20.11+, curl, unzip, and sha512sum:

```sh
./scripts/setup.sh                  # official pinned engine + matching templates
./scripts/setup.sh --graphics       # also unpack verified graphics tools on Debian 13 x86_64
./scripts/run.sh                    # requires a working Vulkan display
./scripts/test.sh                   # synthetic Python/Node + headless Godot tests
./scripts/capture.sh                # private isolated X11/Vulkan screenshot
./scripts/export.sh Linux           # build/linux/lost-heaven-reborn.x86_64
./build/linux/lost-heaven-reborn.x86_64
```

The exported game runs independently of the editor and analysis tools on a host
with writable user data and a Vulkan driver. In this restricted cloud shell,
`source scripts/common.sh` before starting the exported binary supplies writable
XDG locations. Local tests use only synthetic fixtures and resources.

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
JSON metadata. This validator does not extract archives or load original assets
into the game.

An external extracted PC 4DS v29 model can be converted with the read-only MIT
parser tooling. No npm installation is needed:

```sh
./scripts/convert-model.sh --source /private/model.4ds --textures /private/maps --output /private/new-model
./scripts/inspect-model.sh /private/new-model/model.glb --output /private/new-capture
```

The output directory must be new and its parent must exist. Inputs, textures,
GLB, metadata and captures stay outside the checkout; symlink escapes and
output overwrites are refused. Source bytes and permissions remain unchanged.
The converter supports opaque static LOD0 visuals, positive scales and dummy
frames, with uncompressed 8/24/32-bit BMP textures. Missing textures fail unless
`--allow-missing-textures` explicitly permits a labelled color fallback. Skin,
morph, joints, animation, alpha/reflection/additive/emissive materials and other
frame types are rejected. It preserves hierarchy and source metadata, reflects Z,
converts stored WXYZ rotations, and reverses winding. Source units remain at
scale 1: their interpretation as meters is **unverified**; calibrate before
creating meter-based level metadata. Diffuse PBR lighting and nearest filtering
are presentation choices, not an original-game appearance match.

A private original PC crate and its original texture passed GLB validation and
were visually inspected using this converter/viewer in Godot 4.6.3 Mobile/Vulkan
on llvmpipe. Source and imported bounds agreed within floating-point tolerance.
The public tests use generated models and bitmaps only. Headless model checks
prove structure and transforms; the inspection command additionally records the
actual renderer and a private screenshot. It reuses the bounded capture cleanup
and leaves the synthetic game unchanged. No original-game comparison, skeleton,
animation, district or original-car gameplay has been validated.

[COVERAGE.md](COVERAGE.md) records implemented systems, evidence levels,
remaining binary analysis and fidelity gaps.

Next: extract a small PC district with scene/collision placement metadata and
validate an original character's skin/rest pose and 5DS timing against private
reference captures. REA and Ghidra binary analysis of the provided PC game are
required for the faithful reconstruction; tool installation alone does not
validate behavior. Wine and PS2 emulator reference runs depend on host feasibility.
Full engine/game coverage, original-runtime comparisons and Apple hardware checks
remain outstanding.

Our original code is MIT licensed; game assets and trademarks remain with their
owners. See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for inspected references.
