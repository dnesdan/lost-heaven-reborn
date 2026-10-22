# Reference-project audit

Audited on 2026-10-22 against Rust engine commit [`a739570`](https://github.com/dnesdan/lost-heaven-reborn/commit/a739570). These pinned source audits identify reconstruction leads and incompatible assumptions. They add no native comparison cases, completed functions or gameplay credit. Detailed source hashes, line references and findings remain in the private research ledger.

| Reference | Audited revision | Useful evidence |
| --- | --- | --- |
| [LS3D-SceneKit](https://github.com/alex-alex/LS3D-SceneKit) | [`8fadd220`](https://github.com/alex-alex/LS3D-SceneKit/tree/8fadd2201f52936d998c66c47b0a0d37476c2ce5) | 4DS skin/morph layouts, 5DS channels, scene light fields and TCK/DAT sidecars. |
| [LS3D_FaceAnim](https://github.com/kirill-mapper/LS3D_FaceAnim) | [`a25706b2`](https://github.com/kirill-mapper/LS3D_FaceAnim/tree/a25706b29970887c6d0ef2ab34f22c262f9d6ebd) | Six facial parameters, ten regional morph weights and editor import/export behavior. |
| [mafia-re](https://github.com/Marvisak/mafia-re) | [`86a7b15c`](https://github.com/Marvisak/mafia-re/tree/86a7b15cbfa002bc6222e5542671a81988472882) | Original-address anchors, PC/PS2 layout distinctions, Joint/singlemesh dependencies and newly added LitObject reconstruction. |

## LS3D-SceneKit

The [skin reader](https://github.com/alex-alex/LS3D-SceneKit/blob/8fadd2201f52936d998c66c47b0a0d37476c2ce5/Mafia/Models/ModelMesh.swift#L47-L158) provides concrete leads for inverse bind matrices, bone indexes, rigid/blended vertex counts and two-influence weights. The [morph reader](https://github.com/alex-alex/LS3D-SceneKit/blob/8fadd2201f52936d998c66c47b0a0d37476c2ce5/Mafia/Models/ModelMorph.swift#L14-L119) exposes vertex links and per-LOD payloads. These need bounded Rust intake and original deformation comparisons; our current owned graph supports mesh/dummy frames, without Joint or skeletal deformation.

Several presentation choices cannot establish original behavior. Missing parents fall back to the root, only the first LOD is rendered, and some recognized visual types have no payload implementation. Material animation and lightmaps are skipped. [Scene lighting](https://github.com/alex-alex/LS3D-SceneKit/blob/8fadd2201f52936d998c66c47b0a0d37476c2ce5/Mafia/Scene/Scene.swift#L5027-L5062) uses tuned intensity scales. Our current unlit renderer also remains approximate; original driver, material, sector and visibility contracts are still required.

A specific incompatibility is [5DS interpolation](https://github.com/alex-alex/LS3D-SceneKit/blob/8fadd2201f52936d998c66c47b0a0d37476c2ce5/Mafia/Animations/AnimationLoader.swift#L387-L420): SceneKit uses smoothstep, while our original-code comparisons establish linear position interpolation in the supported domain. Keep that verified path. Rotation, scale, notes, TCK root motion and audio-linked facial timing require separate recovery. No Apple application execution or SceneKit dependency was introduced by this audit.

## LS3D_FaceAnim

The complete two-file repository and both commits were inspected. Its [format description](https://github.com/kirill-mapper/LS3D_FaceAnim/blob/a25706b29970887c6d0ef2ab34f22c262f9d6ebd/README.md#L37-L58) and validator describe two header words followed by eight-byte records: six parameters and two trailing bytes. Import discards the trailing bytes; export writes `1,1` and canonicalizes the second header word. A future Rust reader should preserve all raw fields and validate lengths rather than assume those bytes are irrelevant.

The [weight equations](https://github.com/kirill-mapper/LS3D_FaceAnim/blob/a25706b29970887c6d0ef2ab34f22c262f9d6ebd/LS3D_FaceAnim.ms#L436-L503) map six bytes into ten weights across three regions. Each region sums to one; the editor multiplies weights by 100 for Max Morpher. This is a useful test reference, but native operation order and rounding remain unverified. The tool supplies neither joint/topology loading nor model compatibility checks beyond existing morph channels.

Mafia's 25 FPS and H&D2's 50 FPS are manual editor settings, not an encoded timing contract. New controllers default to Bezier; optional Step changes interpolation. Retained PC REA identifies candidate file/audio/state processing with 40 ms timing, but incomplete float recovery still needs instruction analysis and original-call probes. No Max execution, original facial playback comparison or H&D2/PS2 compatibility was established.

## mafia-re

Since the earlier pinned reference, the main addition is LitObject plus supporting declarations. Its [render path](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/I3d_lit_object.cpp#L1501-L1595) helps locate lighting-level, vertex/index-buffer and world-space dependencies. The current Rust world-space visual rejection remains until actual original vertex dispatch is verified.

The committed LS3DF card reports 97.60% instruction-comparison progress, with 1,350 of 1,361 functions associated with source. The [metric](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/tools/progress.py#L97-L143) also changed its denominator: 299 linked-library functions were excluded while 103 source-associated functions were added. These figures cannot be substituted for our static inventory, scoped Rust ports or runtime verification. This audit did not rebuild the upstream comparison toolchain.

The [configured LS3DF hash](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/reccmp-project.yml#L1-L34) matches our reference; its Setup targets differ. Game.exe has no implementation target there. PS2 debug names are useful provenance, but PC Joint, scene and animation layouts differ. The [Joint matrix helper](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/I3D.h#L2870-L2902) explicitly retains a nonmatching compilation caveat; missing or duplicate joint indexes also require careful admission rules.

## Next dependency order

| Step | Concrete deliverable | Required original evidence |
| --- | --- | --- |
| 1 | Owned Joint state and parsed joint records. | REA boundaries and constructor/load/update bytes, PC/PS2 layout differences, linked resource invalidation. |
| 2 | Singlemesh joint binding and deformation. | Parent/index/cache rules, missing/duplicate indexes, matrix order, vertex and normal results before GPU integration. |
| 3 | Real actor/model animation ownership and first MISE01 control dependencies. | Actual Game factory/body binding, eight animation channels, lock callbacks, replay, vehicle attachments, camera and collision. |
| 4 | LitObject, material and sector rendering. | Original resource lifetime, vertex dispatch, lighting/visibility state and controlled reference images. |
| 5 | Morph/DAT/TCK and remaining animation channels. | Raw corpus preservation, original interpolation, target geometry, time boundaries, audio cursor and fade behavior. |

Each deliverable remains independently authored Rust, checked through REA and original execution, with two sequential reviews. Full first-mission gameplay and complete engine/game compatibility remain open; external source coverage does not close those gaps.
