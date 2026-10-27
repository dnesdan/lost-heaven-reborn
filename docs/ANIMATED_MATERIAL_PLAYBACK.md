# Animated material playback

This implementation follows pinned `mafia-re` revision `eb5a921`,
`ls3df/I3D/mesh/I3d_material.cpp` constructor, `UpdateTexture`,
`UpdateEnvTexture`, `Duplicate`, `SetCurrentFrame` and `SetEnvCurrentFrame`,
and the lazy accessors in `I3D.h`. It is source-backed implementation and
synthetic validation, without new native-function credit or original-runtime
parity claims. The source file SHA-256 is
`0923eea7d945d56c33811a88bad0aa89eebb564afc4a7e80210ef7a225cd65d7`;
`I3D.h` is `676fc980457c6faa7475662bb6c682bba9f6f70b42167153caac2da5b5ef9115`.

## Live ownership and access

`VisualResources` owns each color/environment frame reference. Current texture
fields are borrowed aliases; playback does not retain/release references.
`ModelArena::set_material_driver_time(i32)` publishes the host's wrapping
millisecond driver timeline and propagates it before later material admission.
Publication advances no materials. Admission preserves an existing driver clock
through `CanonicalSceneOwner::open_at_driver_time`,
`CarRuntimeOptions::material_driver_time`, or
`SceneBuilder::set_material_driver_time` before appending a selected model.
The canonical snapshot calls the actual
material getter for each reached textured face group, in draw order, using the
arena's existing LS3DF CRT stream. The current renderer does not access the
environment getter, so environment playback remains lazy and is exercised only
by explicit resource access/tests.

Color and environment clocks are independent. Same-tick accesses do not consume
another random value. Random mode consumes one shared CRT value after its full
interval, without iterating over skipped frames. Sequential playback preserves
the source's `loop_frame + frame % (length - loop_frame)` expression, including
its unusual nonzero-loop cycle. The zero-time initialization sentinel and the
separate frame-selection/restart behavior remain visible.

Host guards report unsupported negative delays and invalid sequential loop
bounds only when their unsafe branch is reached. The source environment setter
accepts `frame == length` before its out-of-range access; the host retains its
frame/timing writes and reports that boundary as unsupported. It does not report
native success for that access.

## Retained standalone model adapter

Ordinary animated models selected through original model loading use actual
loaded visual/material resources for both initial draws and later snapshots.
`RetainedScene::snapshot_animated_material_draws(time)` returns each exact
contiguous `draw_range` and its live snapshot. The host replaces this range in
both the renderer and cached Scene. `close_owned_material_draws` performs explicit
model teardown and unused-buffer reclamation before replacement or shutdown.
This adapter has fixed LOD0 topology and original `maps` intake without cropping.
Multiple independent animated retained arenas require a canonical Scene to share
the driver's CRT state; they are explicitly unsupported by this adapter.
Unconstructed lit animation and environment rendering retain their existing
unsupported boundaries.

## Validation

Resource tests cover independent clocks, lazy shared CRT order, repeated ticks,
large sequential deltas, the nonzero-loop expression, 32-bit clock wrapping,
frame selection/restart, constructor-zero behavior, duplicate reset, reached
unsupported boundaries, and final reverse-order frame release.

The integrated CPU fixture loads public synthetic BMP frames, constructs real
material owners, snapshots three face groups sharing two animated materials,
checks random consumption and changed pixels, then destroys the owners and
reclaims buffers. The retained assembly fixture tests the standalone adapter's
initial draw and repeated driver snapshots. The explicitly ignored GPU test
`gpu_canonical_animated_frames_change_rendered_pixels` runs the same canonical
fixture through renderer replacement and RGBA8 readback, checking red pixels
before playback and blue pixels afterward. GPU execution requires a Vulkan or
Metal adapter; the logic suite remains GPU-independent.
