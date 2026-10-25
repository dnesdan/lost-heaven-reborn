# Stage 5d: preserve authored host-view orientation

This bounded presentation repair preserves roll in the existing
`main.rs::authored_camera` consumer. It does not complete the canonical-camera
presentation bridge tracked by audit area 13. No original-runtime or REA
function credit is added.

## Source evidence and coordinate convention

Reference: `mafia-re` commit
`29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c`.

- `ls3df/I3D/frames/I3d_camera.cpp::I3D_camera::UpdateV`, PC
  `0x1000ba30`, constructs the view from all three rows of the camera's world
  basis and its position. Replacing its up direction with world Y would discard
  part of that orientation.
- `ls3df/I3D/I3d_math.cpp::S_matrix::SetRot3`, PC `0x1002e070`, establishes
  the quaternion-to-row-basis signs. For a positive quarter-turn about Z, the
  source up row is +X and forward row is +Z. For a positive quarter-turn about
  X, the source up row is -Z and forward row is +Y.
- `ls3df/I3D/frames/I3d_frame.cpp::UpdateWMatrixProc`, PC `0x1001c7d0`,
  composes a parented world matrix as local times parent world. This general
  frame rule does not establish the metadata object's active-camera association.

The existing host `source_rotation` converts source WXYZ to the conjugate
glam XYZW quaternion. This expresses the source row-vector basis as host
column-vector directions. `authored_camera` now rotates both +Z (forward) and
+Y (up), then applies the same `(x,y,z) -> (x,y,-z)` presentation reflection
to both directions and position. The renderer's existing `look_at_rh` therefore
retains roll and accepts a vertical forward direction when its authored up
direction is perpendicular. Existing quaternion, coordinate-range and basis
validation still applies.

## Actual caller and remaining boundary

The existing scene2 metadata selection feeds this camera directly into window
rendering and capture. Selection by object name, local position/quaternion,
missing-object inspection fallback, fitted clipping range, provisional vertical
50-degree FOV and metadata retention behavior are unchanged.

`lhr-assets/src/scene.rs` documents this parser as metadata derived from
`mafia-workbench`. The pinned `mafia-re` has no `intro_pos` association or
implemented Game scene2 loader establishing this object as the original active
camera. This change therefore does not infer parent transforms, original
controller scheduling or mission ownership. It does not connect `CameraFrame`
main/near/backdrop matrices, scene viewport, or the separate source rendering
passes to the host renderer. Audit area 13 remains open.

## Verification

Synthetic fixtures exercise the actual `authored_camera` path through loose
`Installation` reads, scene2 parsing, name lookup and retained metadata. They
check a quarter-turn roll, source-to-host reflected position/basis, the rolled
view's screen-up direction, and the formerly rejected vertical-forward case.
Zero/nonunit/nonfinite quaternion and invalid host positions remain errors
without fallback or metadata retention. These are CPU logic checks; no original
assets, original-runtime image comparison or interactive verification is used.

Validation uses the separate `target-stage5d` directory. The three focused
tests pass. The mandatory `scripts/test.sh` passes formatting and workspace
check, then reports only the two pre-existing x87 high-FIP-address failures:
`preserves_long_mode_x87_pointers_across_actual_helpers` and
`preserves_full_fxsave64_state_with_pending_exception_and_live_sse`.
Repeating the workspace test command with exactly those two tests skipped
passes: assets 121, installation 8, engine 1055, binary 29, program 8, renderer
5; existing ignored tests remain ignored. Python host boundary tests pass 9/9,
progress tests 47/47, and the generated-progress check passes. Workspace
all-target Clippy with `-D warnings` passes.
