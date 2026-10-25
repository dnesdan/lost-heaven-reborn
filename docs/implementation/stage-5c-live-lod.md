# Stage 5c: live owned LOD selection and fade

This stage is stacked on Stage 5b. It connects ordinary source LOD selection to
actual owned levels, the car/driving snapshot, the current host camera and the
renderer refresh. No native REA function credit or original-runtime comparison
is added.

## Source contract

Reference: `mafia-re` commit
`29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c`:

- `ls3df/I3D/render/render.cpp:117-183`, GetObjectLOD: squared distance divided
  by `(RSLodQuality * world_scale)^2`; select the first inclusive threshold,
  an open-ended last level, or the one-past-last render-cull sentinel. The
  ordinary branch stores cur_lod on the same object. Single missing/nonpositive
  level early returns precede distance/scale arithmetic and the cur_lod store.
- `render.cpp:300-306`: RSLodQuality is RS_LOD_QUALITY times
  `(PI/2)/camera_horizontal_fov`. `frames/I3d_camera.cpp:118-125` confirms that
  the source FOV is horizontal. The host renderer's centralized vertical FOV
  is converted using the actual current viewport aspect, not reused as a
  horizontal FOV. Main selects the explicit original driver-init quality 1.
- `visuals/I3d_object.cpp:614-670`: distance uses the visual's own world-bound
  sphere center, not the hierarchy center; the final finite-distance LOD fades
  over the source interval, with only the source upper clamp. The final distance
  equality selects that level with fade zero; a greater distance culls it.
- `I3d_object.cpp:559-573`, SetFade: null mesh returns before fade is inspected;
  the new float is stored even when rounded alpha is unchanged. An alpha-byte
  change marks only private slots below the current mesh.num_lods, in reverse
  order. Hidden slots retained after truncation are not marked.
- `visuals/I3d_visual.cpp:114+`, UpdateWBoundProc: transform the own visual AABB's
  eight corners and recompute its world AABB/sphere. HR world bounds remain
  separate and still supply Stage 5b's transparent sorting center.

## Ownership and actual consumers

`I3dObjectResources` retains constructor cur_lod=0 and fade=1 on the same object
that owns its mesh and private LOD slots. The canonical constructor class
witness is Plain; derived owners must extend that witness and provide their
own dispatch. The ordinary selection/fade adapter explicitly requires Plain.
No callback dispatch, Texture::Request or derived-class render is substituted.

The selected view reads mesh.levels[index] and the corresponding private
object.lods[index] override. Actual CPU graphics metadata must prove FVF0x112
(XYZ/normal/UV, 32 bytes). UV-only or unknown layouts fail explicitly. A cull
sentinel never indexes the level array and never falls back to LOD0.

Camera-aware snapshots combine that selected owned geometry with object fade
before Stage 5b's material-alpha packing/classification. Its unlit per-material
scope remains unchanged; this is not PrepareLights' shared lighting contract.
Legacy snapshot wrappers explicitly remain LOD0 diagnostics.

Main car/driving presentation uses the view-aware snapshots. Refresh happens
after wheel/chase/orbit updates and before the actual frame render, using the
current surface configuration. Headless selection uses the same extent as its
capture. Camera bootstrap uses the actual model HR bbox reflected into renderer
space: min Z is -PC max Z and max Z is -PC min Z. It does not require first-LOD
geometry as an invented selection result.

The shared refresh function preserves trailing static-world draws while car
face-group counts change, become zero on distance cull, and later re-enter.
An all-distance-culled car is a valid empty draw set. A car model without any
admitted drawable CPU mesh still fails explicitly; a generic empty scene tree
remains a valid empty scene.

## Limits and validation

The bounded path excludes LOD callbacks, VISF_UNKNOWN_10 texture-driven LOD,
proxy/world-space visuals, derived LitObject rendering, morph/singlemesh and
unknown/UV-only buffer ownership. Raw static-world presentation remains the
legacy path from Stage 5b. Dynamic native driver/camera-global binding, native
texture requests and full lighting are not reconstructed. Host camera FOV
conversion is tied to the actual host projection; no native transcendental
bit-equivalence is claimed. Nonfinite/exceptional exponent arithmetic and
needed zero divisors fail explicitly, without a generic scale guard on source
early-return paths. No source distances are reordered or replaced by guessed
thresholds.

Synthetic tests cover inclusive thresholds, open-ended final levels, scale and
FOV/aspect effects, source fade and owner state, cull sentinel/re-entry, null
mesh and hidden private slots, unknown vertex layout rejection, own versus HR
bounds, and asymmetric translated camera bootstrap. Actual GPU readbacks check
camera-selected geometry/fade and the SAME main refresh helper through count
changes, zero draws and re-entry while trailing world pixels survive. These
are host/source-contract checks, not original runtime screenshots, measured
FPS gains or interactive gameplay validation.
