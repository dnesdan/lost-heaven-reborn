# Original asset intake

`lhr-assets` reads a user's local installation directly; it does not require an extractor, Game.exe, or a game framework at runtime. `Installation::read` checks supported DTA indexes before loose paths. Static REA contract `PC-RUST-MENU-001` records the native `dtaSetDtaFirstForce` flag and startup registration sequence A8, B8, A2, A6, A0, A1, A3, AC, A4, AA, A5, A7, A9 and AB; the Rust reader supports A8, A2, A6, A1, AC and AA in that relative order. AC registration constants and key decoding are also cross-checked against original instructions. This is static evidence, not runtime validation. The first indexed copy wins across archives; a loose file is used only when no supported archive index contains the path. A failed read from the selected source does not fall back to another source, a stricter error policy than the native fallback path. Paths match case-insensitively and archive names use Windows-1250. Duplicate archive names and duplicate resource names within one archive fail. Discovery validates every recognized archive, so a malformed archive can prevent opening even when the requested file exists loose.

Archive lookup is lazy and bounded: indexes have at most 100,000 entries, assets 64 MiB, decoded blocks 32 KiB, entry filenames 255 bytes and scanned directories 100,000 entries. Raw block kinds 0 and 2–7 and custom LZ/RLE kind 1 are supported; codecs 8 and above (audio) are unsupported. Checksums and opaque header fields are not interpreted. The reader does not extract or cache archive contents. Earlier private validation read 20,229 indexed resources and compared bytes against a private prototype decoder, not an independent source extraction or original-runtime comparison. Four known resources also match independent loose-source hashes. The installation root must be a trusted, stable directory tree; checks do not provide descriptor-relative confinement against hostile concurrent replacement of parent directories.

The bounded asset APIs cover static 4DS v29 geometry, BMP and TGA images, Czech/Polish Windows-1250 TextDB strings, GFnt font metrics, language glyph maps, fixed-record menu definitions, scene2 object/actor metadata, and city-cache placement metadata. Private parser checks resolved all 12 menu text IDs against a 5,287-row Czech TextDB and confirmed Rust TGA RGBA output against Pillow for selected original atlases. Frozen Linux X11 captures show the Rust menu and static INTRO/MISE01 inspection flow, but do not establish original menu compositing parity. The Rust application supports the Czech PC layout/localization; profile selection and unsupported menu actions are incomplete. No original font rendering semantics, intro video playback or mission script execution is established.

Scene parsing preserves bounded object transforms, source names, actor kinds, embedded script text and unknown-chunk counts. City-cache parsing preserves model placements, source flags, secondary scale and bounds data; it does not stream the city or infer collision. Static scene loading and the explicit standalone skin/morph routes have separate admission gates. The latter preserve Joint, skin, morph and Target records and require checked owned hierarchy/pose preparation before rendering. Aggregate raw morph metadata limits apply before allocation. Original Tommy static and sampled skeletal poses are supported in a bounded domain; complete animation-controller and resource lifetimes remain open. Alpha, environment, emission, material animation and other advanced effects are counted and omitted from the static renderer. The renderer selects LOD0 and uses an unlit approximation. Source menu rectangles and image assets are used by the UI, while compositing and camera projection remain approximate; the inspection camera uses a provisional 50-degree field of view. Original transform, material, lighting and LOD parity remain unverified.

Embedded actor source can be parsed and inspected. The standalone `inspect_scripts --control-probe` diagnostic supports literal `LABEL`, `GOTO`, `GOSUB`, `RETURN`, nonnegative-literal `WAIT`, `COMMANDBLOCK` and `END` controls in a bounded step. Unsupported commands stop with their source line. This is not integrated with actors, events, replay data, world effects or mission progression; expressions and complete VM behavior remain unsupported.

Private input inspection validated MENU geometry and metadata (86 scene objects, 2 actors), INTRO (358 objects, 28 actors and 16 scripts), and MISE01 (3,183 objects, 293 actors, 39 scripts and 352 cache parts with 10,546 placements). These counts verify bounded parsers against selected original files; they do not mean actors, scripts or the city are executed. The original BMP palette path initializes all 256 palette entries before reading the declared palette count, matching the bounded REA finding. Input files and hashes remain private. Public tests use synthetic fixtures.

## Bounded MRPH scene admission

The explicit `parse_four_ds_scene_morphs` gate admits file visual 5 as a distinct
`I3D_morph` (`MRPH`, `0x4850524d`). It reads the ordinary mesh/instance prefix and
`morphBase` suffix without inventing a skin record. Older static and scene
parser entrypoints keep their visual gates. The live frame retains its source
`Arc<Model>` definition and separate raw/normalized weights; selected-LOD
summation updates the shared canonical CPU mesh and the selected visual's own
managed render-position LOD before lighting and draw extraction. Each render LOD
retains its positions and update count across other file instances' canonical
writes and repeated frames. Lighting keeps its live canonical position/normal
reads; draw extraction borrows the separate owned render positions. Only indexed
morph positions change, preserving untouched private render vertices.
Cache duplication copies current mesh vertices and raw weights while retaining
the shared immutable definition. File mesh instances preserve mesh aliasing.
Definition bounds, update-count/world-bound invalidation, the PC global dirty
flag and derived last-LOD fade remain explicit parts of this bounded path.

The source reference is mafia-re `eb5a9216eea23210e96a3148e351e8211b27e240`.
Private REA/Ghidra inspection uses PC LS3DF SHA-256
`e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6`.
It corroborates the morphBase constructor, Clear, LoadImageB, MRPH constructor
and copied mesh-level vertex storage. The virtual entries have native instruction
and vtable-reference evidence but unresolved Ghidra function boundaries; this
milestone receives no whole-function coverage credit. Proxy/world-space/callback
rendering, arbitrary nonfinite/denormal arithmetic, native allocation failure
prefixes and the complete native render routine remain outside its admitted
scope. This is source-backed Rust implementation, not an original-runtime
comparison.

Synthetic tests cover two selected LODs, shared file-instance writes, independent
cache-copy meshes, retained live weights/definition lifetime, UV preservation,
numeric-domain errors without source-geometry substitution, and final buffer
reclamation. Alias regressions cover the first draw, repeated draws, changing
only one instance's weights, and disjoint partial-region writes. Separately
invoked Vulkan tests verify distinct aliased draw positions, stable repeated
pixels and visible changes when one instance's weights change. The MRPH-focused
CPU run passes six tests, and both explicitly invoked Vulkan tests pass. The initial milestone's required workspace run passed format and build
checks and 1,372 engine tests; two existing long-mode x87 fixture preconditions
failed on this host, and eight tests were ignored by default. Python wrapper tests
pass with process-identity visibility; progress tests and the metadata snapshot
check pass. Workspace Clippy reaches existing renderer documentation warnings.

A private original-data host-render probe retains the complete 30-frame model
and produces 11 draws. Changing the two MRPH owners' live target weights changes
166 uploaded vertices and 365 pixels at 512×512, followed by zero retained graphics
bytes after close/reclamation. The images and asset data remain private. This
exercises the Rust renderer with original inputs; it is not an original-runtime
pixel comparison or interactive/gameplay acceptance.

The original-data INTRO CPU probe now passes the former MRPH intake stop and
reaches Scene2 cursor 39,797 after 28 completed objects, versus cursor 1,907 after
10 previously. Its next reported boundary is a camera `Normalize` numeric-domain
error. Terminal disposal completes with all reported owners, graphics, textures
and atlas allocations at zero. These are rewrite CPU intake/cleanup results;
MissionOpen, actor/script execution, authored objectives and gameplay acceptance
are not evaluated.

## Evidence

REA/Ghidra contracts `PC-RUST-ARCHIVE-001`, `PC-RUST-MENU-001`, `PC-RUST-CONTROL-VM-01` and `PC-RUST-CONTROL-VM-RUNTIME-02`, and the private `PC-RUST-SCENE-001` parser manifest, record bounded findings and input hashes. `scripts/test.sh` passed 61 Rust tests and 9 REA wrapper tests. Frozen menu capture/input evidence is recorded privately in `rust-menu-validation/FINAL.json` and `manifest.sha256`; a separate renderer diagnostic fix preserved the captured pixels and corrected effect counts. The original runtime separately verifies the menu-to-prolog-to-first-mission route, but that result is not Rust behavior evidence. Neither file-format parsing nor original-game reference play establishes Rust campaign compatibility.

## Provenance and license

Read-only layout adaptations in `crates/lhr-assets` draw on MIT-licensed `b4rtt/mafia-workbench`, revision `9f1edbfbc20ab2ac57e66d4f8a444127e92d602e`, including `src/core/fourds.js`, `dta.js`, `textdb.js`, `scene.js`, `cache.js`, `detectors.js`, and format constants. Rust implementations add bounded reads, explicit unsupported cases, strict validation and synthetic tests; proprietary assets, decompiler output and editing tools are not included. The full MIT notice is preserved in [UPSTREAM_LICENSE](../crates/lhr-assets/UPSTREAM_LICENSE). See [third-party notices](../THIRD_PARTY_NOTICES.md) for attribution.
