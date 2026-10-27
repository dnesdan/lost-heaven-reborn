# Parity roadmap after main integration

Updated 2026-10-27. Code baseline: `6e790279dd5d9bf2a111d0a9330527e8bc4b968a`.
This is a work plan, not a claim that the campaign or the listed features are
complete. See the [integration record](2026-10-27-main-integration.md) and the
[Rust boundary migration](../architecture/rust-compatibility-boundaries.md).

## Reference and what it can actually supply

A fresh upstream commit query still returned mafia-re
[`eb5a9216eea23210e96a3148e351e8211b27e240`](https://github.com/Marvisak/mafia-re/commit/eb5a9216eea23210e96a3148e351e8211b27e240),
2026-10-26. There is no newer upstream change in this audit to import wholesale.
Track engine-library parity and complete Game.exe mission behavior separately.

| Reference area | Available evidence | Boundary |
| --- | --- | --- |
| [Mission Open/Close/OpenBin](https://github.com/Marvisak/mafia-re/blob/eb5a9216eea23210e96a3148e351e8211b27e240/game/mission/Mission.cpp) | Ordered scene opening, weather reset, check/road loading before OpenBin, cache/progress/audio handling, actor factory, deferred links and teardown sequence. | `C_mission::Tick` is still a stub; available orchestration does not implement every callee. |
| [C_game](https://github.com/Marvisak/mafia-re/blob/eb5a9216eea23210e96a3148e351e8211b27e240/game/mission/game.cpp) | Some stack cleanup and video-volume behavior. | `Init`, `Done` and `DoneIntern` are stubs. |
| [C_program](https://github.com/Marvisak/mafia-re/blob/eb5a9216eea23210e96a3148e351e8211b27e240/game/mission/C_program.cpp) | Source ownership, patch intake/replacement and teardown. | The rest of script compilation/execution is not implemented here. |
| [Camera](https://github.com/Marvisak/mafia-re/blob/eb5a9216eea23210e96a3148e351e8211b27e240/game/camera/g_camera.cpp), [menu](https://github.com/Marvisak/mafia-re/blob/eb5a9216eea23210e96a3148e351e8211b27e240/game/ui/g_menu.cpp) | Useful camera and menu event/traversal logic. | Requires real player/vehicle/collision/profile/component dependencies; the host preview is not equivalent. |
| LS3DF loading/rendering | Detailed behavior for many resource, visual, light, sector and render operations. | A Rust parser or isolated helper is not proof that the real update/draw path uses it. |

For available bodies, port their behavior through typed owners and compare
observable effects. For stubs or unresolved dependencies, use REA and original
PC instruction/runtime evidence. PS2 linker names help identify functions but
do not establish equivalent PC layouts or platform behavior.

## Validation prerequisite

The execution host was offline (`tunnel_client_not_seen`). The integrated code
has not received a new Cargo, Clippy, GPU, original-data or interactive run.
Before using it as a validated baseline:

```sh
scripts/test.sh
cargo test --locked -p lhr-engine --all-features mission_actor_stream
cargo test --locked -p lhr-engine --all-features mission_scene_loading
cargo test --locked -p lhr-engine --all-features lab_owner_inspection
cargo test --locked -p lhr-engine --all-features lab_camera
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
```

Use the repository's pinned toolchain/setup. Also check formatting, the documented
explicit GPU cases, native Engine Lab pause/step/reload and narrow camera clipping
ranges. Run original-data probes locally with the same input hashes and capture
reached boundaries and cleanup results. None of these commands is recorded as
executed by this audit.

The two historical x87 high-FIP prerequisite failures remain unresolved; do not
silence them or count a filtered supplemental run as a green required gate.

## Work packages, in dependency order

### 1. Complete the common mission loading and close path

Compose the existing owners under the original Open sequence: previous mission
close, scene intake, weather/group state, optional `check.bin` and `road.bin`,
cache detection, sound-volume handling, OpenBin, cache admission and final state.
Preserve optional-versus-corrupt input distinctions, live callback reads and the
actual order of effects. Do not reintroduce a second inspection-only world.

Finish missing general loader arms rather than skipping chunks to reach actors:
visual factories, deferred light-sector links, weather and navigation/road roots,
and remaining versioned object fields. Deferred light and sound sector requests
need the original shared ordering, not two independently reordered passes.

Use the original Close sequence while retaining explicit host terminal cleanup
for interrupted operations. Do not replace unknown `game.Done` behavior with a
successful no-op. Bind the populated navigation owner to the actual installation
stream before calling it production navigation.

**Acceptance:** supported synthetic missions exercise the ordered common path;
original probe frontiers move for a named general capability; every acquired
owner is either released or explicitly retained after injected failures. No
replayed destructor/free and no hidden skipped dependency.

### 2. Finish the actors required by actual missions

Use the original sparse factory and per-class contracts. Prioritize the classes
reached by MISE01, then representative later missions. The recorded kind-27 stop
is a prioritization signal, not permission to map it to a generic base actor.
Connect the existing vehicle implementation through the same mission-owned actor
and frame; add the missing player/derived actor dependencies in their real order.

Migrate numeric handles and object offsets behind typed accessors while doing
this work. Keep source frame userdata, actor collections, ownership and lifecycle
consistent; do not make an independent entity merely to satisfy a script lookup.

**Acceptance:** supported actor creation, LoadData, initialization and teardown
have real effects over the shared scene, including wrong-type and interrupted
construction cases. Every required first-mission actor has an explicit status.

### 3. Execute original programs and the mission loop

Build the compiler/expression/parameter dependencies, typed VM values, calls,
events, waits and scheduling. Keep the existing startup patch table and source
owners. A VM register is legitimate script state; an x86 register/address in a
public gameplay API is not a replacement for it.

Implement GameInit and mission ticking over real actors, inputs, simulation,
programs, camera and sound. Unknown instructions or callees must remain visible
boundaries, not successes that advance the instruction pointer without effects.

**Acceptance:** an original program resolves a real actor, changes its state,
waits on an actual condition and resumes correctly. Pause, repeated ticks and
failure paths preserve ordering. No handcrafted substitute for MISE01's script.

### 4. Restore player-facing camera, controls, menu and HUD

Use upstream camera/menu behavior where implemented, and reconstruct missing
collision correction, player transitions, profile bindings and component callbacks.
Complete the original menu-to-mission transition and HUD events using live game
state. Keep Lab orbit/chase and environment previews explicitly separate.

**Acceptance:** actual profile inputs drive the player/vehicle; the camera handles
obstacles and mode transitions; HUD values follow the same world. New Game,
failure/retry and exit follow the correct lifecycle without leaked owners.

### 5. Close rendering, animation and audio integration gaps

Prioritize ordinary dynamic lighting and sector fog, then environment mapping,
animated material consumers, UV-only LOD and unsupported animation combinations.
Continue with projectors/shadows, mirrors, particles/lens flares and original
weather. Keep transparent ordering, visual flags, bounds and per-instance geometry
consistent through every render pass.

For audio, extend spatial/sector behavior, volume policy, timing, Doppler/reverb
and voice prioritization as evidenced. Complete production music/video plumbing
rather than counting a mocked call sequence as playback.

**Acceptance:** each feature has a representative synthetic scene, a live update
and draw/mix path, and an original-data case. Compare draw state/pixels or PCM and
lifetime traces. Separate software Vulkan, native GPU and original-game evidence.

### 6. Complete objectives, AI, cutscenes and persistence

Use real navigation/road/collision data for traffic, pedestrian/police behavior
and mission interactions. Complete the script effects that drive authored goals,
cutscene synchronization, fail/restart and checkpoint restore. Preserve references
when actors or programs are recreated; avoid opaque raw-address savegames in the
new runtime unless an original file format requires a decoder at its boundary.

**Acceptance:** the original first mission starts from the menu, follows its
original objectives and reaches original completion; failure, restart and
checkpoint restore also work. Driving around a rendered city is insufficient.

### 7. Broaden content and platform coverage

Repeat with multiple original missions and combinations: interior/exterior,
closed/open portals, night/weather, multiple animated actors, cutscenes, repeated
scene transitions and retained-error cleanup. Profile real hot paths before
optimizing or introducing a large framework.

Exercise native Linux Vulkan and macOS ARM64 Metal/audio. Differential-test the
portable numeric backend against the contained x87 reference before changing
its default use. Cross-compilation and software Vulkan alone are not native
macOS validation.

**Acceptance:** reproducible receipts identify source/input hashes, platform,
commands, outputs and remaining differences. Keep original assets local; attach
actual permitted captures to visual PRs without calling old captures new tests.

## Regression inputs, not hardcoded feature branches

The last recorded original-data probes belong to the earlier #38 source freeze,
not to the new main runtime:

| Input | Historical boundary | Next check |
| --- | --- | --- |
| `mise01` | 3,183 Scene2 objects, then actor kind 27 | Implement the evidenced class and downstream dependencies. |
| `intro` | 28 objects, then camera normalization | Expand the numeric domain with evidence, not a default identity rotation. |
| `00menu` | Model animation `menugl`; retained interrupted cache | Complete the general animation/controller path and its cleanup. |
| `mise02-saliery` | World visual type 4 | Implement its actual visual/resource behavior. |
| `mise03-morello` | Frame-name domain restriction | Rerun after #39; the next reached boundary is not yet measured. |

See [the historical evidence](parity-continuation.md) for exact source scopes.
Do not update coverage or first-mission gates from a parser admission, planned
migration, source review or merge alone. Track each feature as loaded, owned,
executed, visibly/audibly effective, then compared with the original.
