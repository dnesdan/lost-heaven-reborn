# Saved vertex lighting

This is an explicit source-backed resource and presentation dependency. It does
not complete scene2 lightmap loading or the original renderer. The reference is
[mafia-re at 29b65b0](https://github.com/Marvisak/mafia-re/tree/29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c).

## Supported path

`ModelArena::load_frame_saved_vertex_lighting(frame, bytes)` operates on an
already-owned genuine LMAP visual. It consumes the saved stream's mask and packed
15-byte headers, accepts vertex modes 1 and 5, checks the actual mesh LOD and
vertex counts, and owns each completed LOD's ARGB words. Its returned cursor
leaves trailing bytes with the caller. Mode 2 rectangle lightmaps are rejected.

The hardware vertex-color branch supplies a real managed private buffer:
FVF 0x142, stride 24, XYZ at byte 0, ARGB at 12, and UV at 16. The existing live
inspection bridge borrows and validates that buffer and its same-owner colors,
selects the actual LOD, and copies both into GPU presentation data. Native buffer
creation is lazy; this host bridge creates buffers eagerly for inspection.

The admitted presentation profile is hardware MODULATE2X with an effective
opaque base texture: `RGB = min(2 * texture_RGB * vertex_RGB, 1)`. Texture sampling
and filtering operate in the original encoded color space. An sRGB render target
receives the corresponding decoded result so its stored bytes match a Unorm
target. Texture alpha is retained; saved vertex alpha, ordinary material diffuse,
emission, opacity and object fade are not multipliers in this source branch.
Textureless, alpha-blended, additive and color-key materials remain unavailable.

## Ownership and failure

Load destroys only the active lighting first; completed nonactive levels remain
owned. Header scalar mutations and completed LODs survive later parse failures.
Such an owner becomes incomplete and cannot render or retry Load. Parse and
validation failures permit explicit teardown. A backend allocation error may
poison the manager and block teardown: the error retains its primary cause,
cursor, and a separate pending LOD/cleanup diagnostic. Poison is never cleared
to manufacture rollback. Native undefined short-read behavior is not emulated.

## Evidence and limits

All source references below are in the pinned checkout:

- `ls3df/I3D/visuals/I3d_lit_object.cpp:1127–1445`: active Destruct, saved
  headers, Load order, modes, LOD publication and completion.
- `ls3df/I3D/visuals/I3d_lit_object.cpp:1844,1905–1956`: alpha selection,
  vertex-lit material/render branch and MODULATE2X.
- `ls3df/I3D/visuals/I3d_lit_object.cpp:2087–2142`: derived cleanup and
  hardware vertex-color buffer construction.
- `ls3df/I3D/visuals/I3d_object.cpp:247–355`: hardware buffer formats.
- `ls3df/IGraph/IGraph.cpp:1497–1510`: texture stage argument initialization.

Synthetic tests cover payload cursors, selected LODs, reloads, ownership,
partial failures and injected allocation failure. Ignored GPU tests exercise the
actual owner-to-snapshot path with nonwhite texels, distinct ARGB channels and
zero vertex alpha, plus both target formats and raw bilinear filtering.

Game scene2 visual replacement/dispatch still requires its private contracts;
the existing `0x543a4b` Unsupported gate remains. Rectangle atlases, active-level
switching, dynamic lighting and fog are separate dependencies. No original-data
comparison or native function/coverage credit is claimed by this slice.
