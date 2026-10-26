# Reconstruction coverage

**311 accepted scoped entries across 114 reviewed contracts. Strict full-system
gate: 0/12. Whole-function coverage: N/A.** The campaign is not playable and
all six first-mission gates remain open. These measures are separate from the
static retained pseudocode inventory and are not a completion percentage.

The owned Scene/Camera renderer now culls the road correctly and draws a real
zapad backdrop. Managed saved-lightmap VB/IB resources, UV2 and atlas leases,
typed Singlemesh/Singlemorph/Billboard/Lensflare owners, initial animated
material pose, and actual Dummy/Sound/AREA lifetimes are connected. A real
MISE01 diagnostic loaded all 3,183 objects, reached actor dispatch at cursor
3,794,014 and produced 267 draws with `Close Ok`. This is a loader and scene
inspection result, not full SceneOpen or MissionOpen.

The Game control/key-mask, camera, four definition tables and load/save order
helpers are bounded dependencies. Active player/HUD/profile, actor
registration, scripts and mission scheduling are still open. General GPU fog,
environment effects, time-evolving animated materials and native image/audio
parity are also open. [Current evidence and limits](docs/implementation/mafia-re-remediation.md)
records the tested paths: 1,561 Rust/56 Python tests, 13 Vulkan checks, 48
feature-enabled Lab tests and two independent applied-source reviews. The
explicit `Models` probe reaches the actor boundary and disposes all tracked
owners to zero; the GPU sky contribution changes 136,934 pixels and restores
byte-identically.

Linux software Vulkan has been exercised. Native Windows/Vulkan and
macOS/Metal have not. Recent PC REA receipts are static instruction evidence;
no original executable or function was run in this milestone. The twelve
system gates remain open across world streaming, rendering, animation,
collision/physics, vehicles, characters/combat, traffic/pedestrians,
AI/navigation/police, scripts/cutscenes, camera/input,
audio/UI/localization and save/load. [Goals](docs/GOALS.md) define the
first-mission target. The full preceding page is [preserved verbatim](docs/history/2026-10-26-pre-final-documentation.md#coveragemd).
