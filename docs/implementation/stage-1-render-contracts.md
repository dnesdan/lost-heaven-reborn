# Render contract corrections — 2026-10-27

Reference: [mafia-re `eb5a9216eea23210e96a3148e351e8211b27e240`](https://github.com/Marvisak/mafia-re/tree/eb5a9216eea23210e96a3148e351e8211b27e240).
This is source-backed implementation and synthetic verification. No original
binary or original runtime is required or credited by these changes.

## Selected LOD fade

`I3d_singlemesh.cpp:593-601` starts the final LOD fade at half the last
squared-distance limit plus the full previous limit. Singlemorph inherits this
render path. Ordinary Object uses half of each limit. The live resource owner
now dispatches using the actual visual class; Billboard retains its already
separate interval and Lit retains its no-SetFade path.

The loaded-resource regression uses limits `[100, 400]`, selects level 1 and
checks both the returned and owned fade. At distance squared 300 the derived
classes retain 1 while ordinary Object produces approximately 2/3; at 350 the
respective results are 1/2 and approximately 1/3.

## Backdrop submission

`render.cpp:493-516` submits the backdrop only while the primary sector has
`SCTF_VISIBLE`. Its earlier CullSector pass remains unconditional. The Scene
snapshot now applies that submission gate after visibility processing and before
material/geometry capture. A closed synthetic interior still culls its backdrop
but submits no backdrop draws; an outward portal connects it to primary and
makes the same backdrop eligible for submission.

## First face-group packed alpha

`T_l.cpp:416-425,584-597` reads the first face-group material once for the entire
LOD's packed vertex alpha. Current-group material alpha and flags still select
the queue and blend state. The ordinary/skinned presentation bridge carries the
first group's packed value separately from those current-group decisions.
Billboard and Lit keep their existing dedicated paths.

The synthetic ordinary and skinned integration regression permutes material
order and exercises actual selected-LOD fade. Both groups share the packed
alpha while retaining their independently selected queue and blend state.

## VISF_ZBIAS and portable backend mapping

`render.cpp:870,904` selects `SetZBias(2)` for visual flag `0x100`, otherwise
zero; `render.h:127-139` forwards that integer to D3D8. The live visual bit now
travels through material and null-material draws into the GPU pipeline key.
Each draw selects its own bias and retains the existing cull and depth-enable
state. This also applies to constructed/unconstructed Lit and Billboard draws.

The numeric translation is an explicit **portable D32 policy**, not a claim of
numeric equivalence to the original D3D8 driver. Its coefficient follows
[DXVK `cbd2206d72d3d9dd9941d5d868bb03a858e85f9b`](https://github.com/doitsujin/dxvk/tree/cbd2206d72d3d9dd9941d5d868bb03a858e85f9b):

- `src/d3d8/d3d8_caps.h` defines `ZBIAS_SCALE = -1 / 65535`, using a D16 convention.
- `src/d3d8/d3d8_device.cpp:1388-1391` multiplies integer ZBIAS by that scale.
- `src/d3d9/d3d9_util.h:211-224` uses `2^23` for its D32 float fallback.
- `src/d3d9/d3d9_device.cpp:7066-7083` applies the format scale to the constant bias.

For native value 2 this gives `(-2 / 65535) * 2^23`, rounded to the integer
constant **-256** required by wgpu. Slope and clamp are zero. The explicit
`Depth32Float` format guard prevents another format from silently reusing this
coefficient. DXVK's optional extension/vendor-specific representations are not
exposed by this policy. Native bias magnitude, vendor differences and
cross-platform numeric equivalence remain unverified.

Synthetic opt-in GPU tests cover coplanar ordering, per-draw reset, null-group
bias, both front windings, culling rejection and the depth-disabled backdrop
pass. CPU tests separately cover live visual-bit capture and the format guard.
These are backend behavior tests, not original-runtime screenshot comparisons.
