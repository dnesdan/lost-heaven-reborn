# Transcendental single-boundary admission under precision owner — 2026-11-03

Reference code: upstream I3d_math.cpp quaternion paths via the existing
`native_trig::Interval` enclosure design and the `Pc24PrecisionState` owner
policy. No new reference pins; the change completes the designed policy, not
a new deviation. Tested on macOS ARM64 Metal; the rejecting check carries no
platform gate, so the stranded input refuses identically on x86_64.

## Problem

MISE01 headless capture failed at `Model(SkinPose(Frame(
QuaternionFromMatrixUnavailable)))` for exactly one scene frame (key 5512, a
near-X-rotation joint matrix with f32 composition noise). Single-model
captures (TommyTAXIDRIVER skeletal animation) already rendered, so Metal,
SkinPose and the enclosure machinery were all healthy — one genuine input
stranded the whole mission view.

## Cause (measured, not inferred)

`SQuat::try_make_from_matrix` fails with `RoundingUncertain` from
`Interval::rounded()`: the seventh transcendental enclosure is
[0.9999999701976754, 0.99999997019768], whose f32 narrowings are the two
adjacent values 0x3F7FFFFF and 0x3F800000. The true value sits within rounding
noise of that f32 boundary, so no implementation owns the bit — the x87
original just stores whatever its extended arithmetic yields, and a 1-ulp
difference is behaviorally irrelevant in the consuming f32 skinning math.
The failure is cross-platform strictness, not an ARM64 portability gap: the
straddle check has no platform gate, and the final result gate passes these
values on both sides.

## Fix scope

`Interval::rounded()` admits a straddle of exactly one f32 boundary (adjacent
finite narrowings) when — and only when — a precision owner is active: it
records host responsibility via the existing `record_precision(true)` channel
and returns the correctly-rounded enclosure midpoint (verified to be one of
the two sides, finite, and zero-or-normal). Wider spans, infinities,
zero-crossings and ownerless callers still refuse with `RoundingUncertain`.
This mirrors the established transcendental policy (`sin_cos`, `atan2` and
friends already record and proceed); the rounding gate was its incomplete
tail.

## Hard limits

- The admitted bit is a documented coin flip on a genuine boundary hit; bits
  are not pinned for such inputs (the regression test asserts admission,
  responsibility recording and unit norm — never the exact side).
- Ownerless contexts (unit probes without a `Pc24PrecisionState` scope) keep
  refusing, preserving the no-silent-decision rule outside game owners.
- `rounded_pc24` and all other gates are untouched. Independent review
  outstanding.

## Verification

```sh
cargo fmt --all -- --check
cargo test --locked -p lhr-engine --all-features --lib -- mise01_near_rotation
cargo test --locked -p lhr-engine --all-features --lib
cargo clippy --locked --all-targets --all-features -- -D warnings
```

New regression test (`mise01_near_rotation_boundary_admits_under_owner`) with
the genuine scene matrix: ownerless use refuses, an owned run admits with the
raised flag and a unit-norm finite quaternion. Full suite: 1839 passed, 11
ignored, 1 pre-existing ARM64-only `ph_thing_ai` failure. MISE01 headless
capture now completes to GPU readback on macOS ARM64 Metal (bridge,
construction site and city blocks render; inspection view, no HUD). Temporary
frame/matrix probes used during diagnosis were fully reverted.
