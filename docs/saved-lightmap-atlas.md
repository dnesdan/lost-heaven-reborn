# Saved rectangle lightmaps

This source-backed dependency adds saved mode 2 to an already-owned LMAP
visual. It does not complete Game scene2 loading or the original renderer.
The reference is mafia-re commit `29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c`.

## Supported resource and presentation path

`ModelArena::load_frame_saved_lighting(frame, bytes)` accepts modes 1, 5 and 2.
Mode 2 retains actual expanded vertices, vertex/rectangle mappings, face
assignments and a separate managed lit index buffer. It validates the real
mesh counts and returns the consumed cursor without consuming trailing data.
The vertex-only entry point remains restricted to modes 1 and 5.

`ObjectResources` owns the shared atlas registry and each lighting allocation.
The explicit profile is truecolor A8R8G8B8-equivalent RGBA with atlas dimensions
128, 256 or 512 per axis. It implements the source packer, ordering, rotations,
free lists and repeated shrink filter. RGB rectangles require dimensions at
least 4 within the signed source range; single colors occupy 3×3 rectangles.
Checked extents and a 256 MiB input budget bound host admission. Original RGB
survives shrink retries and is released after successful atlas upload.

The hardware vertex layout is FVF 0x242, stride 32: XYZ at byte 0, diffuse at
12, atlas UV at 16 and base UV at 24. The selected live resource view borrows
these buffers and atlas pixels. The snapshot shares one copied atlas per atlas
identity, then the GPU consumes the actual expanded indices and both UV sets.
Host buffers are constructed eagerly; this is not native lazy-buffer coverage.

Presentation requires `snapshot_arena_tree_with_view_and_lighting` and an
explicit `StaticLightmapContext`. The caller selects this sector for diagnostic
presentation. Graph ancestry establishes provenance; this check does not reproduce
native portal/cull traversal or the per-frame `m_pNextVisual` queue used by
[AddSectorFGroups](https://github.com/Marvisak/mafia-re/blob/29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c/ls3df/I3D/render/render.cpp#L204-L210).
Every draw rechecks the actual nearest sector,
its scene association and availability, and an empty sector light list. The
special near sector is unsupported. The first face group's material must have
zero emission and finite ambient values: source `PrepareLights` uses that first
material for the whole level. Emission on a later group alone does not change
this admission. Dynamic lighting, fog and animated mesh UV offsets are outside
this static profile; omitted context never implies an empty lighting world.

Opaque effective base textures use the HWTL single-pass source equation:
`RGB = clamp(2 * base_RGB * clamp(atlas_RGB + dynamic_RGB, 0, 1), 0, 1)`.
Here the verified static context supplies zero dynamic RGB. Both textures are
sampled and filtered in source encoded color space, with output conversion for
sRGB targets. Alpha is base alpha times atlas alpha; the atlas alpha byte is
255 in this profile. Ordinary diffuse, opacity and fade are not multipliers.

## Ownership, errors and validation boundary

Load first destroys only the active lighting. Header mutations and completed
LOD prefixes remain observable after failure; the resource owner becomes
incomplete. Atlas construction precedes postProcess UV/vertex validation, so a
later error releases its real allocation. Shared release whitens and reclaims
its rectangles; the last reference removes the atlas. A failed construction
retains a typed pending allocation until explicit cleanup. Backend failure may
poison the manager and block cleanup; the error reports the primary cause and
separate pending-cleanup diagnostic instead of clearing poison or faking rollback.

Synthetic fixtures exercise payload cursors, expanded indices/UVs, atlas sharing,
reload/cleanup, partial failures, source resize pixel oracles and lighting-context
admission. GPU tests require an adapter and use generated images, including raw
filtering and Unorm/sRGB targets. These checks are not original-data comparisons.
Game visual replacement/scene2 dispatch still stops at the existing `0x543a4b`
Unsupported boundary pending private contracts. Active-lighting switching,
other texture/device profiles and full renderer parity remain separate work.
This dependency claims no native-function or progress-coverage credit.

## Pinned source evidence

- [Load and publication](https://github.com/Marvisak/mafia-re/blob/29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c/ls3df/I3D/visuals/I3d_lit_object.cpp#L1159-L1445); [litLod cleanup, Create and postProcess](https://github.com/Marvisak/mafia-re/blob/29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c/ls3df/I3D/visuals/I3d_lit_object.cpp#L2087-L2298).
- [Shared atlas, packing, conversion and release](https://github.com/Marvisak/mafia-re/blob/29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c/ls3df/I3D/mesh/lightmap.cpp#L72-L941); [source resize filter](https://github.com/Marvisak/mafia-re/blob/29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c/ls3df/IGraph/IGraph.cpp#L3349-L3525).
- [PrepareLights and first-material emission](https://github.com/Marvisak/mafia-re/blob/29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c/ls3df/I3D/render/T_l.cpp#L405-L568); [single-pass material equation](https://github.com/Marvisak/mafia-re/blob/29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c/ls3df/I3D/visuals/I3d_lit_object.cpp#L1951-L1996); [texture-stage arguments](https://github.com/Marvisak/mafia-re/blob/29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c/ls3df/IGraph/IGraph.cpp#L1497-L1510).
