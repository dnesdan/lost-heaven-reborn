# Asset preservation and optional graphics work

Read-only research snapshot, 22 October 2026. This proposal does not change engine behavior or game assets. Native compatibility and the first mission remain the priority. No new models, generated textures, screenshots or original payloads are included.

All 14 supplied DTA indexes opened: **36,311 archive entries**, representing **36,204 canonical paths** with 107 overlaps. These are archive counts, including patch duplicates, rather than a count of resources selected by the current installation loader. The catalog includes 3,321 4DS model files, 2,731 5DS animation files, 79 KLZ files, 9,141 WAV files and 37 OGG files. Indexing does not establish runtime support for those formats.

| Bounded examination | Result and limits |
| --- | --- |
| Rust BMP/TGA decoding | 4,693 of 4,724 files decoded; 31 rejected. Median width and height: 128 pixels; width 2–1,024, height 1–1,024. |
| Texture representation | 3,008 indexed 8-bit, 1,656 24-bit and 29 32-bit decoded files. Only 29 decoded images have nonopaque alpha; separate masks and material flags also control transparency. |
| Other texture containers | 11,750 TEX/DXT containers catalogued and hashed, totaling 356.2 MiB of declared payloads before DTA compression. Header observations are separate from validated image decoding. |
| Rust scene-model parsing | 2,757 of 3,321 files accepted; 564 rejected for unsupported variants or validation failures. This is structural intake, not rendering parity. |
| Existing research model parser | 3,318 files parsed; three files rejected for unsupported versions. 345 contain skins, 346 contain joint frames, and 230 contain morph containers, including 217 with positive target counts. This does not imply Rust animation support. |
| Other bounded checks | 25 city caches and 80 scene descriptions parsed. Audio examination read 48 archive payloads: 24 WAV headers and 24 OGG payload signatures; playback was not tested. |

Research model totals across stored frames and LODs range from zero to 179,068 vertices and 99,123 triangles per file; median 164 vertices and 98.5 triangles. Meshes store up to eight LODs. Skin records contain 8–25 bones; morph containers contain up to eight targets. A representative original character has 862 vertices, 1,025 triangles, 18 joints and five morph targets referencing three specific vertex regions. These relationships make topology changes consequential. Figures include nonrenderable metadata files and large scene files, so they are not a typical visible-object budget.

Start optional enhancements with a local texture cache that preserves texture identity, UVs, tiling, material slots, alpha coverage and animation sequence timing. Deterministic filtering and carefully reviewed upscaling can improve sampling. They cannot recover lost detail. Generated detail is an interpretation requiring explicit opt-in and per-asset review. A color texture does not uniquely determine a physical normal or roughness map; baked shadows and highlights must not be treated as surface relief or lit twice.

Use separate **Original** and **Enhanced** modes. Original mode remains the reference. An owned local override manifest should bind each replacement to the selected original resource hash, conversion version and settings; retain originals and fall back when an override is absent or incompatible. Distribute tools and manifests without original assets. Hash checks matter because patch copies can change topology while retaining the same name.

Keep silhouettes, vertex order, UVs, material IDs, frame hierarchy, joint identity and bind transforms, skin assignment, morph correspondence, attachment points, collision, sectors/occluders, lightmaps and mission anchors unchanged for an automatic compatibility-preserving path. Full remeshing or character retopology requires artist-led UV transfer, rigging, weight transfer, facial morph reconstruction, LOD production and animation/interaction review. A faithful automatic conversion of every asset to contemporary graphics is not established.

The decoded image subset alone occupies **490.8 MiB** as RGBA without mipmaps. Doubling both dimensions would require four times that storage; quadrupling them would require sixteen times it. These are cache-size calculations, not measured resident memory or frame rates. Linux/Vulkan, macOS/Metal and any future iOS target need separate measurements of visible geometry, draw submissions, upload stalls, resident texture bytes and frame time before choosing quality budgets. Preserve streaming and original LOD choices before increasing detail.

Acceptance should require unchanged original-mode output and gameplay traces, manifest/hash integrity, exact topology/UV/joint/morph invariants where promised, alpha and mip seam checks, animation and attachment comparisons, collision/occlusion/anchor parity, and repeatable camera comparisons under original lighting. Enhanced rendering must pass platform-specific memory and frame-time limits agreed from measured baselines. No such graphics-performance or visual-parity result is claimed by this inventory.
