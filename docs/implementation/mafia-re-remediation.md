# Scene/resource remediation scope — 2026-10-26

Validated source object:
[`dddaf539c9b874b9cf7a67d866532d901729df18`](https://github.com/dnesdan/lost-heaven-reborn/commit/dddaf539c9b874b9cf7a67d866532d901729df18),
tree `8d78a1aa3fb24e3dbf286fd1baca96dcccc35ec4`.
The 344-file source/config fingerprint is
`0616fb1ef46c6448cedca8df3e28acd92b186e7d466b808d00cf0d4965f4e5c5`.
Two sequential independent code reviews, including the final cache/retention
supplement, approve the final revised source. These are AI reviews, not human
approvals. The predecessor's complete PR/stage record is
[archived verbatim](../history/2026-10-26-scene-render-predecessor.md#remediation).

## Implemented interaction

- Scene2 creates a genuine LMAP resource and performs ordered Duplicate,
  hierarchy/child moves, map replacement, membership changes and reference
  release. Saved-payload Load uses that current object's real owner; native
  negative returns and host interruptions remain distinct.
- Saved lighting owns its geometry, vertex colors, UV2, atlas placements and
  shared texture lifetime. The renderer consumes those actual resources; the
  unconstructed cyan branch remains separate.
- The owned Scene's active Camera and viewport feed backdrop, main and near
  GPU passes. Portal/occluder visibility supplies their actual draw lists.
- In the measured driving run, nearby cache selections materialize 907 actual
  model instances in the same arena/Scene/MissionCaches. Root poses and the
  replacement of each visual's local scale by the second stored scale preserve
  dummy transforms. Typed physics/current-frame maps synchronize visibility.
  This is bounded host-selector adoption, not original Game
  city-streaming or pool-controller parity.
- Source queue/cull rules retain nullable-material geometry and known-zero
  ordering. The admitted null-first-material ambient/point evaluator reads real
  registered lights. Guarded Scene/CarRuntime owners retain entered failures;
  pending creators precede SceneClose, cache teardown and buffer reclamation.

## Validation partitions

| Check | Current result and scope |
| --- | --- |
| Required `scripts/test.sh` | PASS: format, all-target check, Clippy, 1362 Rust tests, 9 Python boundary tests, 47 progress tests and generated check. |
| Debug and release workspace tests | 1362 PASS in each profile; unchanged x87 tests run without exclusions. |
| Opt-in GPU checks | 1374 tests total with 12 GPU checks on Mesa llvmpipe Vulkan; host upload/draw/readback behavior, not original-game image parity. |
| Actual-data world/scene2 | 1415 frames, 5 sectors, 14 portals; 44 completed scene2 records, then `FRAME_DUMMY` (6), `detector_konec`, cursor 18693. |
| Actual-data driving CPU | 907 actual cache models, 0 unsupported/pending/interrupted placements; 686 chase-view draws, 240 steps, audio and GameDone. Explicit cleanup/reclaim reaches zero tracked resources. |
| Final actual saved-lightmap GPU probe | Same owned Scene/Camera, 140 draws / 11 lighting draws including 7 base247 lit draws. Whitening the actual atlas changes 53 pixels; restoring it restores the entire RGBA image byte-for-byte. Close/reclaim reaches zero. |
| Final headless entry points | Default menu and MISE01 inspection exit 0; pinned source/library/input guards remain unchanged and all task-owned probe processes are reaped. |
| Final interactive application | PASS: W/A/D/S/Space, three reloads and Escape with exit 0; four cleanups each reach seven zero CPU counters. Source/binary guards are unchanged and app/Xvfb/harness processes are reaped. |

Final verification v3 receipt SHA-256:
`6dfdc19cdc03c3f5ce1d091f8baf246d35ec0a47eddebaa6f3ac32e90b0bd5c3`.
Per-process, source, library and input manifests are retained privately.

Reproduction entry points remain:

```sh
./scripts/setup.sh
./scripts/test.sh
cargo test --workspace --release
cargo test --workspace -- --include-ignored
./scripts/run.sh --root /path/to/Mafia
./scripts/run.sh --root /path/to/Mafia --model models/taxi00.4ds --car-driving
./scripts/run.sh --root /path/to/Mafia --mission mise01 --headless --capture /tmp/mise01.png
```

Use the pinned Rust 1.99.1 toolchain and original user-owned assets. The driving
UI uses a window; headless MISE01 remains inspection. Native macOS/Metal and host
audio hardware are unverified. No new video is claimed. The current
[driving capture](../screenshots/2026-10-26-canonical-scene-driving.webp) restores
cache buildings, but road and sky remain dark or absent, with the cause
unresolved. This is a material presentation limitation, not a full-renderer pass.

## Evidence and remaining boundaries

The current source cross-check is [mafia-re 29b65b0e](https://github.com/Marvisak/mafia-re/tree/29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c),
freshly confirmed unchanged. Hash/address-keyed PC REA/Ghidra receipts and actual
PS2 symbol crosschecks remain private. REA source is clean 3d565fa42 / version 6.8;
static analysis used isolated 6.7 because Node 24.19 misses the 24.22.1 minimum.
No original executable/function, binary harness or reference application was
executed for this milestone.

Complete startup/MissionOpen, the remaining scene2 factories, original player
and actor scheduling, scripts/objectives/replay, traffic/AI and save/load remain
open. General lighting/fog/environment/specialized passes, remaining driver
view/projection cache/effects, saved-lightmap variants and native streaming are
also open. Source tests, actual-data CPU runs, GPU pixels and UI input do not
establish whole-function or complete-system equivalence.

All six first-mission gates remain open. Accepted work stays **311 scoped
entries / 114 contracts**, strict **0/12**, whole-function coverage **N/A**. This
milestone adds no native-function, ledger or original-runtime parity credit.

[Coverage](../../COVERAGE.md) · [progress](../PROGRESS.md) ·
[goals](../GOALS.md) · [reference audit](../REFERENCE_PROJECTS.md) ·
[documentation history](../history/README.md).
