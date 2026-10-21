# Vendored read-only parsers

Source: [b4rtt/mafia-workbench](https://github.com/b4rtt/mafia-workbench),
revision `9f1edbfbc20ab2ac57e66d4f8a444127e92d602e` (MIT).
The complete upstream copyright and permission notice is in [vendor/LICENSE](vendor/LICENSE).

- `vendor/fourds.mjs` adapts `src/core/fourds.js`: keeps the reader and
  `parseFourDs`; omits writers and their material-settings import; replaces
  iconv-lite decoding with Node's built-in Windows-1250 `TextDecoder`.
- `vendor/bmp.mjs` retains `parseBmp` from `src/core/bmp.js`; omits writers.
- `vendor/fiveds.mjs` retains the read-only `parseFiveDs` from
  `src/core/fiveds.js`; omits editing functions and replaces iconv-lite decoding
  with Node's built-in Windows-1250 `TextDecoder`.
- `convert.mjs`, `animation.mjs`, `png.mjs` and `cli.mjs` are original MIT project code.

Coordinate interpretation follows upstream `src/ui/transforms.js` and
`src/ui/viewport.js`: conjugate stored WXYZ for column-vector transforms,
then reflect Z from left-handed Y-up to right-handed Y-up. Converted vertex
winding is reversed to retain outward-facing normals. glTF image coordinates
start at the top, so source UVs are retained. These are format interpretations;
physical scale and original-game appearance require separate reference checks.

## Experimental skin and animation conversion

The default static converter remains strict. The experimental API
`convertExperimentalAnimatedModel` requires explicit positive `secondsPerFrame`
and optional `{name, source}` animation clips. No default original clock rate is
inferred. Additional LODs, hidden surfaces, overlapping morph regions, unknown
track flags and ambiguous/unmatched names are rejected instead of omitted.
Numeric note tracks (flag `0x10`) are retained as metadata without executing
events; string/pointer events (flag `0x20`) are unsupported and rejected.

Stored quaternions are scalar-first WXYZ. Conjugating their vector part gives
the rotation acting on column vectors. Reflecting Z then gives glTF XYZW
`(x,y,-z,w)/norm`. Vertices and positions use `C=diag(1,1,-1,1)`; source column
matrices convert as `C*M*C`. Local matrices are `T*R*S`, and a frame's world
matrix is its parent's world matrix multiplied by its local matrix.

For skin owner `O` and joint `J`, the original skin matrix is checked against
`inverse(world(J))*world(O)` with maximum coefficient tolerance `1e-4`. Each
source rigid run uses weight 1 for its joint. Each weighted run uses source
weight `w` for its joint and `1-w` for its recorded parent; owner/root is joint
zero. Runs must cover every vertex, weights must be finite in `[0,1]`, and each
source joint ID must bind uniquely below the owner. The target skin includes
the owner followed by source joints. An identity surface node beside the
original hierarchy avoids applying the owner's transform twice: rest skinning
gives `world(J)*inverse(world(J))*world(O)*v = world(O)*v`.

The source type10 frame's separate 16float matrix is **not** identified as a
skin inverse; it is retained with opaque semantics. All morph targets are
retained as independent region/target shapes. A target delta is the source
absolute position/normal minus the base position/normal for that region;
non-region vertices have zero delta. This does not implement original regional
mixing or speech.

Every supported 5DS transform track becomes a glTF channel; raw frame keys,
values and numeric notes remain in output metadata. Local replacement with
LINEAR vector/slerp rotation interpolation is provisional. If a transform
track's last key precedes the declared final frame, an explicit repeated-value
endpoint preserves the chosen clip duration. Original raw keys remain intact;
holding the final value is a documented playback convention. Name matching uses
unique NFC/lowercase names as an explicit conversion convention. PC static
analysis establishes `file frame * 40` in duration and key getters, but does
not establish seconds or controller interpolation. Private evidence dossier
`PC-ANIMATION-API-DOSSIER` links actual REA/Ghidra duration getter `0x10004a20`,
rotation getter `0x10004b30`, position getter `0x10004c50`, scale getter
`0x10004db0`, numeric event getter `0x10004f50` and pointer event getter
`0x10005050`. Original binaries, decompiled output and evidence files remain
private; none are copied into this implementation.

The Godot4.6.3 viewer resolves
`EditorSceneFormatImporter.IMPORT_FORCE_DISABLE_MESH_COMPRESSION` through
ClassDB (value 64) and passes it to `GLTFDocument.append_from_file`. It fails if
the flag is unavailable. This avoids otherwise silent imported vertex
quantization. The viewer imports at 100fps and displays rest pose only. Import resampling can
approximate a clip whose chosen key times do not align with that rate. Numerical
checks against original data establish asset preservation and the selected
playback interpretation only. Original PC runtime and PS2 behavior comparisons
remain separate prerequisites for engine-fidelity claims.

Disabling mesh compression does not remove Godot4.6.3's reduced skin-weight
precision. Private checks observed imported weight and weight-sum differences
of approximately `1.53e-5`. Numerical posed-vertex probes use Godot's CPU
`bake_mesh_from_current_skeleton_pose`, whose delta-blending behavior is
accounted for separately; these probes are not GPU vertex readback. Rendered
images provide visual inspection, not a numerical original-game comparison.
