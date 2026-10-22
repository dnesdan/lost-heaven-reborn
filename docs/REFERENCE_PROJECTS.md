# Reference-project audit

Pinned source audits began on 2026-10-22 against Rust engine commit [`a739570`](https://github.com/dnesdan/lost-heaven-reborn/commit/a739570). Subsequent independently reconstructed Joint/singlemesh behavior is summarized below. Reference audits themselves add no native comparison cases, completed functions or gameplay credit; detailed hashes and findings remain private.

| Reference | Audited revision | Useful evidence |
| --- | --- | --- |
| [LS3D-SceneKit](https://github.com/alex-alex/LS3D-SceneKit) | [`8fadd220`](https://github.com/alex-alex/LS3D-SceneKit/tree/8fadd2201f52936d998c66c47b0a0d37476c2ce5) | 4DS skin/morph layouts, 5DS channels, scene light fields and TCK/DAT sidecars. |
| [LS3D_FaceAnim](https://github.com/kirill-mapper/LS3D_FaceAnim) | [`a25706b2`](https://github.com/kirill-mapper/LS3D_FaceAnim/tree/a25706b29970887c6d0ef2ab34f22c262f9d6ebd) | Six facial parameters, ten regional morph weights and editor import/export behavior. |
| [mafia-re](https://github.com/Marvisak/mafia-re) | [`86a7b15c`](https://github.com/Marvisak/mafia-re/tree/86a7b15cbfa002bc6222e5542671a81988472882) | Original-address anchors, PC/PS2 layout distinctions, Joint/singlemesh dependencies and newly added LitObject reconstruction. |

## LS3D-SceneKit

The [skin reader](https://github.com/alex-alex/LS3D-SceneKit/blob/8fadd2201f52936d998c66c47b0a0d37476c2ce5/Mafia/Models/ModelMesh.swift#L47-L158) provides leads for inverse bind matrices, bone indexes, rigid/blended counts and two-influence weights. Independent PC reconstruction now admits bounded visual-type-2 skin records, Joint hierarchies, dense binding, eager bounds and CPU deformation. One original `civil02` first-LOD pipeline matches 3,124 u32 fields against original execution; 80 of 82 strict models produce static first-LOD poses, totaling 37,948 vertices. Two stop at numeric-domain checks. The [morph reader](https://github.com/alex-alex/LS3D-SceneKit/blob/8fadd2201f52936d998c66c47b0a0d37476c2ce5/Mafia/Models/ModelMorph.swift#L14-L119) provided leads for visual-type-3 loading, vertex links and per-LOD payloads. Independent REA/original-code reconstruction now loads the actual Tommy morph/Target model and renders bounded skeletal pose samples; native resource lifetime and full controller playback remain open.

Several presentation choices cannot establish original behavior. Missing parents fall back to the root, only the first LOD is rendered, and some recognized visual types have no payload implementation. Material animation and lightmaps are skipped. [Scene lighting](https://github.com/alex-alex/LS3D-SceneKit/blob/8fadd2201f52936d998c66c47b0a0d37476c2ce5/Mafia/Scene/Scene.swift#L5027-L5062) uses tuned intensity scales. Our current unlit renderer also remains approximate; original driver, material, sector and visibility contracts are still required.

A specific incompatibility is [5DS interpolation](https://github.com/alex-alex/LS3D-SceneKit/blob/8fadd2201f52936d998c66c47b0a0d37476c2ce5/Mafia/Animations/AnimationLoader.swift#L387-L420): SceneKit uses smoothstep, while our original-code comparisons establish linear position interpolation in the supported domain. Keep that verified path. Rotation, scale, notes, TCK root motion and audio-linked facial timing require separate recovery. No Apple application execution or SceneKit dependency was introduced by this audit.

## LS3D_FaceAnim

The complete two-file repository and both commits were inspected. Its [format description](https://github.com/kirill-mapper/LS3D_FaceAnim/blob/a25706b29970887c6d0ef2ab34f22c262f9d6ebd/README.md#L37-L58) and validator describe two header words followed by eight-byte records: six parameters and two trailing bytes. Import discards the trailing bytes; export writes `1,1` and canonicalizes the second header word. A future Rust reader should preserve all raw fields and validate lengths rather than assume those bytes are irrelevant.

The [weight equations](https://github.com/kirill-mapper/LS3D_FaceAnim/blob/a25706b29970887c6d0ef2ab34f22c262f9d6ebd/LS3D_FaceAnim.ms#L436-L503) map six bytes into ten weights across three regions. Each region sums to one; the editor multiplies weights by 100 for Max Morpher. This is a useful test reference, but native operation order and rounding remain unverified. The tool supplies neither joint/topology loading nor model compatibility checks beyond existing morph channels.

Mafia's 25 FPS and H&D2's 50 FPS are manual editor settings, not an encoded timing contract. New controllers default to Bezier; optional Step changes interpolation. Retained PC REA identifies candidate file/audio/state processing with 40 ms timing, but incomplete float recovery still needs instruction analysis and original-call probes. No Max execution, original facial playback comparison or H&D2/PS2 compatibility was established.

## mafia-re

Since the earlier pinned reference, the main addition is LitObject plus supporting declarations. Its [render path](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/I3d_lit_object.cpp#L1501-L1595) helps locate lighting-level, vertex/index-buffer and world-space dependencies. The current Rust world-space visual rejection remains until actual original vertex dispatch is verified.

The committed LS3DF card reports 97.60% instruction-comparison progress, with 1,350 of 1,361 functions associated with source. The [metric](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/tools/progress.py#L97-L143) also changed its denominator: 299 linked-library functions were excluded while 103 source-associated functions were added. These figures cannot be substituted for our static inventory, scoped Rust ports or runtime verification. This audit did not rebuild the upstream comparison toolchain.

The [configured LS3DF hash](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/reccmp-project.yml#L1-L34) matches our reference; its Setup targets differ. Game.exe has no implementation target there. PS2 linker names provide provenance, while PC Joint, scene and animation layouts differ. The [Joint matrix helper](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/I3D.h#L2870-L2902) retains a nonmatching compilation caveat. Our contract uses actual PC instructions and original execution, with explicit admission rules for missing/duplicate joint indexes.

PC `GetVertexJoint` at LS3DF RVA `0x54dd0` has 2,040 admitted native comparisons. Its boundary was absent from the legacy static inventory and is now admitted by a fresh exact-body audit. This batch preserves its earlier runtime evidence without adding a new scoped-port credit. The current audited LS3DF union contains 764 of 1,255 pinned upstream candidate addresses as exact entries; 491 still require correspondence. These address counts are discovery aids, not Rust semantic coverage.

## Next dependency order

| Step | Concrete deliverable | Required original evidence |
| --- | --- | --- |
| 1 | Scoped result: owned Joint records/state, hierarchy and cached matrix evaluation. | PC instructions and original constructor/load/update/matrix comparisons; separately evidenced PS2 names/layout differences. |
| 2 | Scoped result: dense visual-type-2 binding, eager bounds and first-LOD CPU position/normal deformation integrated into static inspection. | Original skin corpus: 142 models, 261 LODs, 4,698 bones, 51,389 weights / 682,925 exact bytes. Actual `civil02` composed pipeline: 3,124 exact u32 fields. Native allocation/render lifetime and skeletal animation remain outside this result. |
| 3 | Tommy's visual-type-3 model, skeletal rotation tracks, AC animation resources and real actor/model ownership, then first MISE01 control. | Original singlemorph skin/morph loading, name-to-track binding, AC/5DS access and catalog bootstrap, Game factory/BODY binding, eight channels, lock/replay/vehicle/camera dependencies. Caller-owned collision filters and instrumented crouch queries do not supply collision geometry. |
| 4 | LitObject, material and sector rendering. | Original resource lifetime, vertex dispatch, lighting/visibility state and controlled reference images. |
| 5 | Morph/DAT/TCK and remaining animation channels. | Raw corpus preservation, original interpolation, target geometry, time boundaries, audio cursor and fade behavior. |

Each deliverable remains independently authored Rust, checked through REA and original execution, with two sequential reviews. Full first-mission gameplay and complete engine/game compatibility remain open; external source coverage does not close those gaps.

## Direct PS2 symbol crosscheck

The supplied USA `SLUS_206.71` executable (SHA-256 `adf4944c5cdac23fcc4e9af9ef3ec1ba6234497e57cec2af2ff1a7a15c62a46f`) contains original function/global linker symbols, but no DWARF or STABS type/member information. mafia-re also distinguishes original symbols from guessed field names. The earlier batch checked 27 complete symbol ranges. This batch captures 22 singlemesh-related ranges plus two supplemental ranges; the captures overlap earlier work and are not an additive unique-function count. All are static evidence against unchanged original bytes, without PS2 execution credit.

| Original PS2 name | PS2 address | PC correspondence and evidence limit |
| --- | --- | --- |
| `I3D_joint::UpdateV` | `0x42b750` | LS3DF RVA `0x1ee30`; matching invalidation role, different masks and offsets. |
| `I3D_joint::LoadImage` | `0x42b850` | LS3DF RVA `0x1ef10`; PS2 reads a four-byte suffix, PC reads 68 bytes. |
| `I3D_singlemesh::DBuild` | `0x456f00` | LS3DF RVA `0x54c20`; PS2 zeroes joint-pointer holes, actual PC instructions leave unmatched slots unwritten. |
| `I3D_joint::GetJointMatrix` | `0x456cc0` | LS3DF RVA `0x56ba0`; matching cache/traversal and multiplication order, different storage and local-matrix dependencies. |
| `I3D_visual::UpdateV` | `0x45bf70` | LS3DF RVA `0x5a390`; inherited by singlemesh on both platforms, with different invalidation masks. |
| `GetAnimIndex(unsigned int)` | `0x117a80` | Game RVA `0x8760`; instruction-level agreement on unsigned keys, duplicate midpoint selection and null/miss behavior. |
| `C_human::Do_Crouched` | `0x17b2a0` | Game RVA `0x17f8a0`; structural correspondence, with additional PS2 collision-shape stores. |
| `C_human::Do_Shoot` | `0x1802f0` | Game RVA `0x183590`; matched action-kind branches and animation resource references. |
| `C_human::PersonAnim` | `0x16cf10` | Game RVA `0x173e50`; matched code flags, lookup and resource-binding structure. |

These are original PS2 names with separately established PC correspondences, not recovered PC symbol tables. Member labels remain research names. PC model animation has eight channels; PS2 has three. PS2 skin assignments use two bytes per vertex and a secondary weight divided by 256; PC uses per-joint float weights for the primary influence. PS2 CPU position output has stride 16; PC position/normal output has stride 24. PS2 joint discovery returns early at nested singlemesh/singlemorph visuals, stopping the current sibling scan; PC keeps traversing. Inherited visual UpdateV clears PS2 mask `0x140000` versus PC `0x1c0000`. These differences remain explicit alongside the array-hole and Joint-layout differences above.

REA currently rejects the Emotion Engine R5900 variant with `architecture_unsupported`. The PS2 crosscheck used the retained dedicated EE Ghidra backend in read-only mode; PC REA and original-code comparisons remain separate. Future contracts record native binary/address, exact symbol, name origin, mapping evidence, Rust symbol and validation independently. This crosscheck adds no completed-function credit.
