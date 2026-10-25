# Stage 8a: unconstructed lit objects

This bounded source implementation admits the constructor state of
`I3D_lit_object`, retains its base mesh and private geometry, and connects it to
actual live-arena, view-aware presentation. A separate static SceneBuilder
bridge applies the same constructor-state color selector to its existing host
geometry pipeline. Constructed lightmaps and vertex lighting remain unsupported.
No original-runtime, REA function, or generic progress credit is added.

## Pinned primary source

All source references are to Marvisak/mafia-re commit
`29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c`:

- `ls3df/I3D/load/load4ds.cpp:50–53` maps serialized visual ordinal 1 to
  `I3D_VISUAL_LIT_OBJECT`; `ls3df/I3D/I3D.h:87` defines the distinct `LMAP`
  class FourCC `0x50414d4c`. Its file geometry is inherited object mesh data.
- `ls3df/I3D/visuals/I3d_lit_object.cpp:40–64` constructs lighting zero as
  active and six unconstructed lighting records: mode `LITMODE_LIGHTMAP` (2),
  resolution 5, angle 45 and ten null LOD pointers each. The derived destructor
  deletes and clears each lighting's private LODs before base object cleanup.
- `I3D.h:2904` gives the inline `litLod` constructor. The inherited buffer is
  initially empty; `update_count` is zero and `dirty` is true. Unconstructed
  lighting has no lit vertices, lightmap, rectangles, remapping arrays, private
  index buffer or position buffer to release.
- `I3d_lit_object.cpp:1584–1640` selects the mesh LOD with `GetObjectLOD`,
  lazily creates its private `litLod`, and queues the source mesh face groups.
  It has no ordinary object's last-LOD `SetFade` block. `S_vector::Square` in
  `ls3df/I3D/I3d_math.h:109` uses `(x*x + y*y) + z*z`, matching the admitted
  ordinary distance expression and checked PC24-profile arithmetic.
- `I3d_lit_object.cpp:2121` calls inherited `lod::Create` with
  `LODC_TEX2 | LODC_POSITIONS | LODC_STATIC`, source positions and source base
  UVs when there are no constructed lit vertices. `I3d_object.cpp:247–352`
  and `ls3df/ls3df.h:378` establish the hardware-T&L layout: FVF `0x242`,
  `VF_LVERTEX2` (1), stride 32, XYZ at 0 and primary UV at 16. Diffuse and
  second UV storage are not written by this branch and are not consumed here.
- `I3d_lit_object.cpp:1844–1914` selects alpha argument 1, disables stage 1,
  sets the opaque cyan factor `0xff00ffff`, and either modulates the base
  texture by cyan or selects the factor alone when there is no texture. The
  material still controls clamp and culling. Its ordinary diffuse, opacity,
  fade and emissive color do not replace that factor.

The textured branch is **not universally opaque** in the source. Its add or
texture-alpha flags choose the transparent queue; add and texture-alpha with
nonzero emissive choose additive-alpha blending; color-key chooses alpha test.
The current admission explicitly rejects these textured branches and separate
alpha masks before asset reads or owned creation. With no base texture, source
rendering resets alpha state to none, so those alpha flags do not create an
ordinary opacity/fade operation. An assigned material is required: the native
null-material branch inherits cull state, which this host bridge does not guess.

## Ownership and actual consumers

`I3dObjectResources` carries a constructor-owned `ObjectVisualType` witness.
The optional boxed `LitObjectResources` keeps six lighting records without
increasing every ordinary object's storage by their sixty slots. Base object
LOD slots remain plain-only. Unconstructed lit LODs own their actual managed
vertex allocations through the inherited base-buffer arena; they use separate
lighting-zero slots and share only the mesh's actual face-group index buffers
and materials. Host intake prepares these buffers eagerly, rather than claiming
native lazy rendering. Zero host bytes in unused diffuse/UV2 fields are not
claimed original data.

Close preflights the actual LOD identities, then releases derived buffers in
lighting/LOD order before the ordinary ten slots and mesh reference. Existing
poisoned-prefix semantics stop cleanup after an error; explicit Close remains
required. Ordinary class witnesses used by collision and vehicle consumers
reject the lit subtype. Lit instances, mesh replacement, dynamic vertex updates
and active-lighting changes remain outside this bounded lifecycle.

The live-arena snapshot resolves actual selected mesh levels and private lit
buffers, including retained owner/block/count/base/graphics identity and the
actual manager FVF/stride/format. It never reads UV-only or unknown storage as
lit positions. `select_render_lod` shares `GetObjectLOD` but returns lit fade 1
without calling ordinary `SetFade` or changing base alpha-dirty state. Actual
main view-aware snapshots use this same dispatch; the legacy diagnostic entry
still explicitly selects LOD0. Material selection uses cyan and preserves raw
texture alpha for source `SELECTARG1`; ordinary diffuse/opacity/fade are not
baked into pixels. Plain and lit upload views have separate texture identities
when their alpha preparation differs.

Static SceneBuilder remains a host metadata/constructor-state view. It uses its
existing transforms, crop/retained bindings and decoded texture ownership,
selecting the shared cyan policy with original-driver-init material context.
Its swapped triangle winding uses clockwise front faces, while the live-arena
path preserves source indices and uses counterclockwise front faces. Empty
textures use white times cyan. Lit-only ignored material scalars do not trigger
ordinary-effect rejection; a material also used by ordinary geometry retains
that ordinary guard. Unsupported lit flags, animation, unassigned groups,
instances and texture-driven/world-space branches fail before scene charges or
reads. Selected lit image decode failures do not become diffuse fallbacks.
This does not claim canonical saved-lighting ownership for SceneBuilder.

## Explicit remaining boundary

`I3d_lit_object.cpp:1159–1444` loads version `0x21` lighting records, rectangles,
RGB maps or vertex colors, remapping vertices, indices and face-group rectangle
assignments. `litLod::postProcess` at 2200+ creates/packs the LightMap and derives
second UVs. Those payload owners, construction algorithms, scene saved-lighting
association, alternate lighting selection and constructed render branches are
not reconstructed. No parser/helper is added which pretends to support them.
This change supports the observable unconstructed fallback, not full audit-area
8 lightmap compatibility. Full geometry is required; UV-only mesh levels and
other visual subclasses keep their existing explicit boundaries.

## Verification

Synthetic fixtures exercise model intake, distinct class witnesses, all six
constructor records, private XYZ/UV layout, real Close retirement, material
admission failure prefixes, and the actual SceneBuilder output. A two-level
live-arena fixture proves selection of a different private buffer and topology,
near-level equality, final culling, and unchanged lit fade/alpha-dirty state.
A separate explicit Vulkan/Metal test uploads the actual view-aware snapshot
and reads cyan with no texture and texture times cyan, including raw alpha 17.
The existing host upload samples sRGB texture data into a linear Unorm target;
its expected readback is documented as that host behavior, not an original-game
image comparison. No original assets or interactive gameplay were exercised.
