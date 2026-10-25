# Stage 4a: missing-axis matrix-to-quaternion Make

## Scope and source contract

This change completes the missing-axis branch of the existing
`SQuat::try_make_from_matrix` consumer path. Evidence is the public source at
`mafia-re` commit `29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c`:

- `ls3df/I3D/I3d_math.cpp:1053-1103`: `S_quat::Make(S_matrix const&)`.
- `ls3df/I3D/I3d_math.cpp:772-775`: `S_matrix::GetScale`.
- `ls3df/I3D/I3d_math.h:101-126`: `S_vector::Cross` and `Magnitude`.

When the existing scale-product test detects a degenerate basis, Make copies
the matrix. Missing X/Y/Z rows use original rows Y cross Z, X cross Z, and
X cross Y respectively. In particular, the Y cross order is retained. Every
cross reads the original matrix, including when multiple rows are missing.
A reconstructed row whose Magnitude is below the source threshold receives
its corresponding unit-axis fallback. The copied matrix's scale is recomputed;
a still-small scale product returns identity, otherwise Make recurses into the
existing nondegenerate path. The caller's matrix is never modified.

The implementation reuses checked PC24 arithmetic, row-magnitude cutoffs and
the existing scale-product association. Numeric admission is checked before
fallbacks or recursion, so an exponent-domain failure cannot silently become
an identity or unit axis. The recursive call has the same checked product
condition already established as nondegenerate.

## Integration and limitations

The existing `I3dFrameGraph::assign_scale` and lazy `update_matrix` calls now
consume these results, including their original cache-flag and row-scaling
order. No new API or fabricated frame state replaces those consumers.

This branch is source-grounded. No new native REA function credit or
original-runtime equivalence is claimed. The source itself records remaining
compiler-level differences around Cross/term order. Existing finite normal-or-
zero arithmetic admission, transcendental rounding boundaries, and platform
limits remain in force. `UnrecoveredMatrix` remains available for genuinely
unsupported nondegenerate numeric paths; NaNs, wider x87 exponent-edge inputs
and other unsupported arithmetic do not receive fabricated quaternions.

Collision audit item 17 is intentionally a later dependent change. The
world-space mesh/singlemesh guard in `vehicle_frame_crash` remains intact.
Moving it past hierarchy bounds requires a matching, proven world-space
visual/hierarchy bounds producer and its invalidation contract. Constructor
sentinel bounds are not a substitute. This PR does not claim that collision
boundary is closed or that the playable mission gate is complete.

## Validation

Synthetic logic tests cover all three single missing rows, the original Y
cross sign, multiple missing rows reading original rows, parallel-row fallbacks,
the Magnitude cutoff, the second scale-product identity exit, unchanged source
storage and checked exponent rejection during reconstruction. Frame-consumer
regressions cover both scale assignment and lazy update, resulting local/world
matrices, preserved translation and stable caches. Existing failure-prefix
tests now use a genuinely unsupported subnormal input instead of a supported
missing row.

These are source-contract and consumer logic checks. Rendered/interactive
validation and comparison with the original runtime have not been performed.
