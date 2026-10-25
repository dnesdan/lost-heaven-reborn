# Stage 1: 4DS version 29 source compatibility

Reference: local `Marvisak/mafia-re` checkout pinned to
`29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c`. This stage uses the
user-authorized source-only audit route. It adds no binary reconstruction,
original-runtime comparison, function-credit or progress-credit claim.

## Source contracts

- `ls3df/I3D/frames/I3d_light.cpp`, `I3D_light::LoadImage`, reads the common
  frame prefix and then 40 bytes from mode through cone_out. `I3D/I3D.h`
  describes the two integer words and eight float words. Intake preserves every
  bit, including nonfinite floats. Model loading uses the existing light
  constructor, loads common fields, and copies this suffix directly. It does
  not call setters or fabricate derived caches; normal later ownership and
  transform updates retain their existing behavior. Unsupported runtime light
  kinds still fail at the existing update boundary. Rendering lights and fog
  evaluation remain outside this change.
- `ls3df/I3D/mesh/I3d_material.cpp`, `I3D_anim_material::LoadImage`, reads
  two one-byte counts, then four signed 32-bit fields: loop frame, frame delay,
  environment loop frame and environment frame delay. The retained suffix is
  18 bytes. Animated material evaluation remains unsupported.
- `ls3df/I3D/load/load4ds.cpp`, `LoadMeshImage`, uses vertex count `0xffff`
  for an array of UV coordinates with LOD0's vertex count. This level has no
  vertex mesh and receives no `UpdateBBox`. Intake therefore preserves the UV
  array separately, keeps its vertex array empty, and validates its own face
  groups against LOD0's count. A first-level sentinel is rejected because no
  base level exists. Decoded storage quotas include UV arrays. The current
  resource materializer explicitly rejects UV-only levels before allocation;
  it does not invent positions, normals, vertex counts or bounds. Existing
  first-LOD presentation remains unchanged.
- The existing BMP/TGA decoder path now accepts either extension in base,
  alpha and environment material names. The same relative-path checks apply
  to both formats. Static scene assembly checks format-specific dimensions before
  charging its decoded image budget, then uses the matching decoder and the
  existing optional diffuse fallback policy. This does not expand the decoder's
  supported TGA encodings.
- `I3D/I3D.h` defines `MATF_KEEP_PIXELS` as `0x8000`;
  `I3d_material.cpp::GetTextureFlags` maps it to `ITC_KEEP_PIXELS`. Strict
  static intake and inspection diagnostics no longer classify it as a shader
  effect. The existing texture request flag mapping stays intact.

## Acceptance evidence

Synthetic public tests cover exact light-suffix alignment, signed animation
fields and following geometry, retained raw light float bits and constructor
caches, model light hierarchy and lifetime, repeated UV-only LODs, face and
material bounds, malformed/truncated records, safe mixed-case TGA names, and
keep-pixels admission/diagnostics. Build and test results belong to the PR's
verification record; no native or interactive validation is implied here.
