# Stage 5b: explicit presentation material decisions

This change is stacked on Stage 5a's persistent resource cache. It implements
source-derived draw decisions in the host renderer and connects the actual live
arena/car material bridge. It does not add native REA function credit or claim
original-runtime image parity.

## Source contracts

Reference: `mafia-re` commit
`29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c`.

- `ls3df/I3D/visuals/I3d_object.cpp:816-849`: queue classification. A group
  without a selected base texture and with full object/material alpha is opaque,
  even when its material flags would select an additive draw state. Otherwise
  texture alpha/add mode or reduced alpha chooses the transparent queue;
  color-keyed opaque groups follow plain opaque groups.
- `I3d_object.cpp:955-969`: independent blend/alpha-test and two-sided/cull
  decisions. Nonzero emissive component bits select additive alpha when the
  normal transparency branch is reached; emission lighting itself is separate.
- `ls3df/I3D/I3D.h:4266` and `ls3df/IGraph/IGraph.cpp:1494`: blend enables alpha
  testing with reference 1; the comparison is GREATER_EQUAL. Color-key-only
  material uses the configured reference. `game/Main.cpp:728` sets it to 200;
  the bridge selects that game-initialization value explicitly.
- `ls3df/I3D/render/render.h:15-42`: SrcAlpha/InvSrcAlpha and SrcAlpha/One use
  the same factors for color and destination alpha.
- `ls3df/I3D/render/T_l.cpp:419-422,690-699`: PrepareLights uses level flags
  and the first face group's material; VertexLightingProcess separately uses
  per-material TEXTURE/KEEP_DIFFUSE flags to select white versus material
  ambient/diffuse. This bridge implements an explicit per-material unlit scalar
  following the latter selection rule. It does not reconstruct PrepareLights'
  level aggregation, shared vertex lighting or first-group color/alpha contract.
  The source packs vertex alpha from material alpha times object fade and
  transparency; this bridge admits those object factors/lightness equal to one
  and packs its per-material alpha under round-to-nearest-even.
- `ls3df/I3D/frames/I3d_portal.cpp:214-226`: transparent sort distance comes
  from the world hierarchy bound center's signed view-plane depth, offset by
  10000 and scaled by 1000. It is not radial distance from the camera.
  `render/render.cpp:834+` draws opaque groups first and transparent groups
  in reverse sorted order, without disabling regular-object depth writes.
- `ls3df/I3D/mesh/I3d_material.cpp:98-121`: SetTexture recalculates material
  texture/alpha/add flags from the effective texture. Serialized asset flags
  are not interchangeable with a live material's flags.

## Renderer and actual bridge

`Mesh::material` explicitly distinguishes legacy host draws from original
material decisions. Original variants select alpha testing, blending and
sidedness per draw. All regular original variants keep depth writes enabled
and use LessEqual depth testing. Per-draw uniform parameters remain separate
from the immutable Arc texture cache: different diffuse/opacity values share
one GPU texture/bind group. Material refreshes update parameters and the sort
center without duplicating textures or replacing compatible geometry.

The live `car_inspection::snapshot_arena_tree` bridge reads the current
`I3dMaterial`, selected texture and actual hierarchy-world-bound center from
the existing owners. Diffuse is no longer baked into copied texture pixels.
KEEP_DIFFUSE, packed material opacity, independent queue/blend state and
sidedness reach the renderer. Driving and steering refreshes update those
per-draw decisions alongside geometry.

Handedness conversion is explicit. The live car bridge reflects Z and retains
source indices; its front face is counterclockwise in the host renderer.
The existing assembly/world conversion also swaps triangle indices, so its
corresponding front-face mapping would be clockwise. Authored LH-front normal
and front/back-camera GPU fixtures validate both mappings. The raw assembly
and parsed static-world paths remain legacy diagnostic draws in this PR; the
clockwise pipeline is available and tested, but those paths do not acquire an
invented live material/texture-state owner.

Host ordering uses supplied world bound centers and current projected view
depth. Plain opaque groups precede color-keyed opaque groups, then transparent
groups are drawn far-to-near. Equal transparent depths reverse source order.
Native texture UID batching, the offset/scaled integer distance quantization,
radix-sort details, portal sectors and weather passes are not reconstructed.

## Explicit remaining limits

Dynamic IGraph alpha-reference updates, object fade/transparency/lightness
producers, lighting/emissive color, fog,
environment maps, full mipmap/texture-format selection, billboards, morphs and
singlemesh rendering remain separate. Existing bounded color-key/grayscale
mask intake is retained; unsupported non-grayscale masks remain reported.
Native embedded-image alpha/BPP flag selection is still explicitly rejected
by MaterialLoadContext outside its existing admitted managed branch; no such
texture capability is added by these draw-state or grayscale-mask changes.
The host texture sampling color space is unchanged; this is not a D3D8
colorimetric parity claim. Raw assembly/world material effects remain reported
as unsupported. The opt-in legacy inspection cutoff of 0.5 is separate and
cannot override an explicit original material's alpha reference.

## Validation scope

Synthetic GPU readbacks exercise references 199/200, blend alpha zero rejection,
color and alpha blend factors, additive blending, opaque-before-transparent
queueing, reverse depth order, retained transparent depth writes, both index
conversion/cull mappings, two-sided rendering and texture sharing across
material parameters. A live material producer reaches GPU pixels for
KEEP_DIFFUSE and packed opacity. A populated ModelArena-to-snapshot fixture
checks shared unmodified pixels, material alpha/diffuse policy and the genuine
world hierarchy bound center. These are host pipeline/source-contract checks;
no original-runtime screenshot or interactive gameplay comparison is claimed.


## Validation results

Rust 1.99.1 with isolated target-stage5b:

- `scripts/test.sh`: formatting and workspace/all-targets check passed. Engine
  library: 1,055 passed, one opt-in GPU test ignored, and only the two established
  high-FIP x87 baseline environment failures. No repository test was disabled.
- Complete workspace tests with only those two test names excluded: assets
  library 121 passed; native installation integration 8 passed; engine library
  1,055 passed/one GPU ignored; engine binary 26 passed; program integration
  8 passed; renderer 5 passed/three GPU tests ignored. Doc-tests passed.
- Renderer tests with `--include-ignored`: 8 passed, including actual GPU
  readbacks. Material producer tests with `--include-ignored`: 2 passed,
  including the producer-to-GPU KEEP_DIFFUSE/packed-opacity readback.
- Workspace/all-targets Clippy with `-D warnings`: passed.
- Python process-boundary tests with host process visibility: 9 passed.
  Python progress tests: 47 passed; generated progress check passed unchanged.

The available Vulkan device is Mesa llvmpipe (LLVM 20.1.2), a software device.
GPU pixel checks verify this host pipeline's decisions. They are not captures
from the original renderer, lighting comparisons, FPS measurements or an
interactive gameplay validation.
