# Reference-project audit

## Current static-model cache scope — 2026-10-25

Code `a171632` reconstructs bounded cache miss/hit/copy/growth and advances the
actual-data constructor-Scene probe to43 records/seven entries and missing
`base247`. Original world SceneOpen and loading UI remain dependencies.
New cache/native loader parity is unverified; adjacent retained comparisons keep
their narrower scope. [Evidence and preceding record](history/2026-10-25-model-cache-loader.md).
Observed mafia-re `1ee42a56` and behavior pin `86a7b15c` remain unchanged.

## Current occluder/light scope — 2026-10-25

Code `41365c0` advances the actual-data Rust scene loader through seven records
and an unloaded Model9 to ModelCacheOpen. Current comparisons reuse retained
original observations; ring/HR/light/nonidentity and the new 2086 native prefix
remain unverified. [Scope, proof partitions and preceding startup record](history/2026-10-25-occluder-light-loader.md).
Observed mafia-re `1ee42a56` and behavior pin `86a7b15c` remain unchanged.

## Fresh mafia-re inspection — 2026-10-25

Observed [`1ee42a56`](https://github.com/Marvisak/mafia-re/tree/1ee42a56214afeb54c536b85a4b16b5c4f4d86e3), nine commits/81 files beyond `109954d6`.
All 27 new unique annotated addresses have original-PC primary receipts and
bounded Rust callers/owned-state implementations; this is not whole-function parity.
The behavior pin remains `86a7b15c`; supplied-PC CheckCD returns true and the
upstream drive/retry tail is unreachable. See the [17-family frontier and evidence
limits](history/2026-10-25-original-startup-mission-audit.md). Older audits follow unchanged.

## Previous source audit — 2026-10-25

Rechecked all three requested projects against Rust baseline `261ff0d`.
Repository metadata and branch heads were freshly fetched; inspected source and
per-file hashes are retained privately. This was source inspection only: no
reference build, comparison regeneration, REA analysis or original execution.
The Rust ledger remains **311 scoped entries / 114 contracts**, strict **0/12**.
The mafia-re behavior reference stays pinned to `86a7b15c`; observing new source
neither advances that pin nor grants implementation credit.

| Project | Freshly observed revision | Useful contribution and boundary |
| --- | --- | --- |
| [mafia-re](https://github.com/Marvisak/mafia-re/tree/109954d6aaf7dd36f2d9dacb336797c0d062a925) | `109954d6`, master; 17 commits beyond `9b793967` | New `main` body maps menu, intro and MISE01 callers. Mission loading, initialization and the game loop still have explicit stubs. |
| [MafiaUnity](https://github.com/MafiaHub/MafiaUnity/tree/0d7a12f03ce4385173377c3e9f457dcc2e891f24) | `0d7a12f0`, develop; unchanged, archived | Scene resource composition, name references and a separate patch pass. Unity lifetime and prototype actors do not supply native Scene ownership or mission execution. |
| [OpenMF-Archived](https://github.com/MafiaHub/OpenMF-Archived/tree/7167e74d65eeced643aa51fb56bb62b8556d8389) | `7167e74d`, master; archived | Inspectable scene/name/KLZ joining and format utilities. Its abandoned renderer/physics/player prototypes do not replace the existing Rust implementation. |

### New mafia-re findings

The 17-commit delta changes 48 files. The most useful addition is
[`main`'s NEW_GAME path](https://github.com/Marvisak/mafia-re/blob/109954d6aaf7dd36f2d9dacb336797c0d062a925/game/Main.cpp#L150-L179):
close the old mission, open intro, load its collision, initialize and run it,
close it, then open MISE01, load collision, reset new-game state, initialize,
run and close. This is a source map for subsequent Game.exe research.
[`GameLoopSingle`](https://github.com/Marvisak/mafia-re/blob/109954d6aaf7dd36f2d9dacb336797c0d062a925/game/Main.cpp#L38-L44),
[`InitSystem`](https://github.com/Marvisak/mafia-re/blob/109954d6aaf7dd36f2d9dacb336797c0d062a925/game/Main.cpp#L350-L365)
and [Mission Open/Close](https://github.com/Marvisak/mafia-re/blob/109954d6aaf7dd36f2d9dacb336797c0d062a925/game/Mission.cpp#L1-L21)
are stubs; Open returns an error and InitSystem returns false. The committed
Game card reports nine matched bodies, not a working campaign. Scene, IGraph
and ITexture bodies are unchanged from the previous observed revision; the
LS3DF delta mainly adjusts compiler-expression matching.

The [Scene constructor](https://github.com/Marvisak/mafia-re/blob/109954d6aaf7dd36f2d9dacb336797c0d062a925/ls3df/I3D/I3d_scene.cpp#L88-L180)
requests `CreateITexture("Snow.tga", NULL, 2, ...)`. This is the immediate,
image-alpha path; managed creation uses `0x40`. The
[filename factory](https://github.com/Marvisak/mafia-re/blob/109954d6aaf7dd36f2d9dacb336797c0d062a925/ls3df/IGraph/IGraph.cpp#L570-L622)
leads to cache matching, retained references, map-directory lookup, Create and
failure Release. Its upstream address is LS3DF VA `0x1006c2c0`; the reached
[Create path](https://github.com/Marvisak/mafia-re/blob/109954d6aaf7dd36f2d9dacb336797c0d062a925/ls3df/IGraph/ITexture.cpp#L223-L310)
is VA `0x10075f50`. These addresses and flag labels remain source leads until
independently checked against original PC instructions and naming evidence.
Our [material adapter](../crates/lhr-engine/src/loaded_visual_resources.rs)
explicitly rejects immediate bit `2` because source-BPP/alpha behavior remains
unresolved. Existing managed-record creation does not close that dependency.

### MafiaUnity and OpenMF findings

MafiaUnity's [MissionManager](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Core/Managers/MissionManager.cs#L33-L88)
loads scene.4ds, cache, scene2 and KLZ; its
[SceneGenerator](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Internal/Generators/SceneGenerator.cs#L42-L200)
registers names before patching references and selectively applying local
transforms. That suggests duplicate-name, external-reference and patch-order
regression inputs. The names are not an original actor registry. Its
[actor dispatch](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Game/ObjectDefinition.cs#L107-L150)
covers Physical, Character and Door; Player, Car and original mission-script
execution are absent from that dispatch. C# mod `ScriptMain` is a separate
mechanism. Unload delegates to Unity object/resource lifetime; there is no
native shared Snow or thirteen-effect Scene Close implementation here.

OpenMF's [mission assembly](https://github.com/MafiaHub/OpenMF-Archived/blob/7167e74d65eeced643aa51fb56bb62b8556d8389/components/mission/mission_impl.cpp#L18-L122)
and [collision-name lookup](https://github.com/MafiaHub/OpenMF-Archived/blob/7167e74d65eeced643aa51fb56bb62b8556d8389/components/mission/mission_impl.cpp#L193-L287)
are useful cross-checks for data relationships. Its
[special-record merge](https://github.com/MafiaHub/OpenMF-Archived/blob/7167e74d65eeced643aa51fb56bb62b8556d8389/components/scene2_bin/parser_scene2bin.cpp#L77-L103)
requires an already-seen base object and drops unmatched specials.
[Character support is TODO and player start creates a generic visual](https://github.com/MafiaHub/OpenMF-Archived/blob/7167e74d65eeced643aa51fb56bb62b8556d8389/components/mission/mission_impl.cpp#L310-L390).
[KLZ conversion](https://github.com/MafiaHub/OpenMF-Archived/blob/7167e74d65eeced643aa51fb56bb62b8556d8389/components/klz/bullet_klz.cpp#L71-L189)
assumes grouped first-link faces and hardcodes cylinder height; the
[renderer](https://github.com/MafiaHub/OpenMF-Archived/blob/7167e74d65eeced643aa51fb56bb62b8556d8389/components/4ds/osg_4ds.cpp#L227-L248)
reduces skin/morph forms to standard geometry. No connected original mission VM
was found in the inspected engine loop. Keep our native-backed collision,
animation and archive behavior; use these projects to challenge missing data
relationships rather than adopt their prototype mechanics.

The next bounded implementation and its acceptance criteria are listed under
[Next general-engine work](#next-general-engine-work).

## Historical source audit — 2026-10-22/23

The following dated observations retain their original implementation limits;
they do not describe the current standalone driving milestone.

Historical source audit refreshed on 2026-10-22 against Rust engine commit [`5903942`](https://github.com/dnesdan/lost-heaven-reborn/commit/5903942266c212c6cd5e88b75df688973e6b447b), extending the earlier audit at `a739570`. Four independent research assignments inspected mafia-re and MafiaUnity formats, runtime and rendering. That historical inspection retained the same mafia-re revision. A source-only refresh on 2026-10-23 observes upstream `0c44be32dd152c9722bbd8c2c8e18f112a279c04`, 19 commits beyond the behavior pin. It adds instruction matching, a Game.exe bootstrap and additional version targets; no upstream build, regenerated comparison or original execution was performed for this refresh. No reference application was built or run, and this refresh performed no new REA analysis or original-runtime comparison. The audit baseline was 110 scoped entries across 41 contracts; the audit itself adds no implementation or gameplay credit. The later bounded frame/model lifecycle snapshot retained 117 scoped entries across 43 contracts. The subsequent resource-primitives snapshot retained 126 scoped entries across 44 contracts. The subsequent mesh-object snapshot retained 129 scoped entries across 45 contracts. The buffer-manager snapshot retained 147 scoped entries across 46 contracts. The subsequent standalone bridge retained 147 scoped entries across 46 contracts with zero new canonical entries. The per-object plain LOD snapshot retained 151 scoped entries across 47 contracts. The nonmanaged texture/string snapshot retained 159 scoped entries across 49 contracts. The managed-removal snapshot retained 160 scoped entries across 50 contracts; managed-record creation retained 161 entries across 51 contracts. The Request-producer snapshot retained 162 entries across 52 contracts. The standalone-unload snapshot retained 163 entries across 53 contracts. The prior release snapshot retained 164 entries across 54 contracts. The Restore snapshot retained 165 scoped entries / 55 contracts. The Seek/Close snapshot retained 167 entries / 56 contracts. The loose-file dtaRead snapshot retained **168 scoped entries / 57 contracts; 0/12 strict full-system gates**. The archive-read extension retained those counts and its 68 original-code READ observations and 32 SEEK context rows. The prior bounded native archive registration/Open milestone retained **176 scoped entries / 58 contracts**, with 24 native opens among 81 log rows and 976 compared bytes. The later bounded PCM16 mono/stereo WAV milestone reached **177 scoped entries / 59 contracts**. Native Write, generic non16 decoding, other channels, full DLL parity and app audio playback integration remain open. Existing creation/removal/Close composition adds no duplicate entry. Current admitted plain-object CPU loaded-resource ownership is distinct from full native texture Create/worker/upload, whole-device shutdown, derived LOD/GPU adapters and the first mission, which remain open. See [coverage](../COVERAGE.md) and [progress](PROGRESS.md).

| Reference | Audited revision | Useful evidence |
| --- | --- | --- |
| [LS3D-SceneKit](https://github.com/alex-alex/LS3D-SceneKit) | [`8fadd220`](https://github.com/alex-alex/LS3D-SceneKit/tree/8fadd2201f52936d998c66c47b0a0d37476c2ce5) | 4DS skin/morph layouts, 5DS channels, scene light fields and TCK/DAT sidecars. |
| [LS3D_FaceAnim](https://github.com/kirill-mapper/LS3D_FaceAnim) | [`a25706b2`](https://github.com/kirill-mapper/LS3D_FaceAnim/tree/a25706b29970887c6d0ef2ab34f22c262f9d6ebd) | Six facial parameters, ten regional morph weights and editor import/export behavior. |
| [mafia-re](https://github.com/Marvisak/mafia-re) | Observed [`9b793967`](https://github.com/Marvisak/mafia-re/tree/9b793967e83e6ce68bfb748a9fb8b7c04e54cbf7); behavior pin [`86a7b15c`](https://github.com/Marvisak/mafia-re/tree/86a7b15cbfa002bc6222e5542671a81988472882) | Original-address anchors, resource lifetime, factories, materials, visibility, collision, animation and explicit PC/PS2 differences. |
| [MafiaUnity](https://github.com/MafiaHub/MafiaUnity) | [`0d7a12f0`, develop](https://github.com/MafiaHub/MafiaUnity/tree/0d7a12f03ce4385173377c3e9f457dcc2e891f24) | Data readers, mission scene assembly and presentation adapters; prototype mechanics require separate native verification. |

**Decision:** use mafia-re to locate original engine algorithms and dependencies; use MafiaUnity to identify data-to-world integration gaps. Preserve the existing native-backed Rust contracts. Neither project supplies a completed original Game.exe runtime or first mission to transplant.

The stream follow-up confirms upstream `0c44be32` is unchanged; rw_data source also matches behavior pin `86a7b15`. Original exported `dtaRead` (RVA `0x25c0`) now has a bounded loose-file branch contract: **168/57**, including the prior Seek/Close entries. Its 27 Wine10/QEMU READ observations exclude archive decoding, native Open and whole-function parity. Snapshot/cache intake is a host adapter. Upstream Lock/Unlock and leak-detector names remain guesses; PS2 symbols do not establish PC member layouts.

The fresh source audit retains upstream `0c44be32`: its README marks Game.exe as started and `WinMain.cpp` implements a startup-directory wrapper at Game RVA `0x166160`, while `Main.cpp` remains a return-zero stub at `0x1bebe0`. No actor/script/mission lookup implementation was found. The inspected LS3DF frame-search dependencies remain source leads, without runtime equivalence or new Rust credit. The real-file host facade at `f04cc43` was a library-only step. At `b856d76`, existing Installation menu/static scene callers now use the selected native archive provider, retaining 177/59 and zero new function credit. Its exact-name/row/owner route bypasses the `dtaOpen` wrapper; AddRef/Release/destructor, Seek and app audio use are not demonstrated. That Installation batch reused primary REA evidence, with no fresh analysis, original execution or new upstream fetch. Its validation remains a historical receipt.

At `5b28728`, Rust C_car construction now feeds bounded primary Init, explicit cleanup and a real initialized-car inspection mode. The accepted scope is **272 entries / 93 contracts**, **21 static_inspection / 72 not_integrated**, strict **0/12**. Whole-function coverage remains N/A.

Actual PS2 linker evidence corroborates C_car::C_car, C_car::Init, C_car::GameInit and C_car_ex::Init; complete PC primary bodies and virtual slots establish their PC correspondences. Native comparisons cover the constructor and admitted primary Init/destructor paths only; GameInit and the derived Init wrapper remain static context without execution claims. The observed mission loader at 0x541030 calls CreateActor at 0x544b02, then primary virtual Init +0x48 at 0x544b12, tests AL and adds the actor on success. This static call-chain evidence adds no complete factory/loader or mission credit. Indexed alpha fragment comparisons establish palette-zero RGB equality for flags 8 and destination format 21; the host 0.5 cutoff adds no native TextureCreate/renderer entry credit.

Latest retained mafia-re source-only observation is `9b793967`; behavior remains pinned to `86a7b15c`. The observation reports six new upstream commits and source changes in math, rendering, texture and build files. No upstream build, original-instruction execution or Rust behavior credit follows from this source audit.

Actual authored MISE01 remains FINDACTOR/UnsupportedWorldBackend at PC 3 / source line 4; no later line executes. All six first-mission gates remain open. No runtime gameplay integration or driving mode is established.

Recover actual C_car::GameInit at 0x41ed00 separately from the observed mission CreateActor → primary Init → AddActor route. Complete Scene viewport/weather/shared Snow construction beyond 0x48e29 and the thirteen actual Scene Close effects with their correct resource identities, then original File Open/reload and mission-world ownership. Source-parameter flags 2/audio remains unavailable.

Historical replay busy/setter and `RECWAITFOREND` evidence remains recorded in [PROGRESS.md](PROGRESS.md) and the immutable replay observation. The pinned/current upstream trees and PS2 linker replay names establish no original PC cinematic-owner correspondence.

## Rust organization and original names

Ownership code uses `lhr_engine::ls3df::{c_str, itexture, i3d_object, texture_manager}` while physical source paths and existing Cargo packages stay unchanged. `I3dObjectResources` is a bounded ownership projection. `Lod` follows actual PC `lod::Create()` diagnostic bytes; `I3D_object` names have original PS2 symbols with independently researched PC correspondence. Member names retain their original-message, source or research provenance; a source spelling alone is not an original symbol.

| Original boundary | Current Rust organization and limit |
| --- | --- |
| LS3DF.dll | Recovered engine/resource components in `lhr-engine`; logical `ls3df::{c_str, itexture, i3d_object, texture_manager}` modules. `lhr-render` supplies wgpu presentation, without full IGraph/Direct3D or audio-library parity. |
| rw_data.dll | Bounded archive/provider behavior in `lhr-assets`; no complete DLL parity. |
| Game.exe | Scoped gameplay policies and the host application share `lhr-engine`; their origin remains separate from LS3DF. |
| IJoy.dll | Host winit input in `lhr-engine` is an adapter, without original input-DLL parity. |
| Setup.exe | Selected shared configuration in `lhr-engine::original_config`; no original Setup executable. |

`lhr-assets` also parses LS3DF/Game formats, so package placement does not identify an original loader or VM. mafia-re combines I3D, IGraph and sound in one LS3DF build target; its newer I3D subfolders are upstream organization, not recovered original packages. The behavior reference and retained address correspondence stay pinned to `86a7b15c`. The latest retained source observation is `9b793967`; its I3D subfolders remain upstream organization rather than recovered original packages. Observing newer source adds no Rust implementation or runtime credit.

## LS3D-SceneKit

The [skin reader](https://github.com/alex-alex/LS3D-SceneKit/blob/8fadd2201f52936d998c66c47b0a0d37476c2ce5/Mafia/Models/ModelMesh.swift#L47-L158) provides leads for inverse bind matrices, bone indexes, rigid/blended counts and two-influence weights. Independent PC reconstruction now admits bounded visual-type-2 skin records, Joint hierarchies, dense binding, eager bounds and CPU deformation. One original `civil02` first-LOD pipeline matches 3,124 u32 fields against original execution; 80 of 82 strict models produce static first-LOD poses, totaling 37,948 vertices. Two stop at numeric-domain checks. The [morph reader](https://github.com/alex-alex/LS3D-SceneKit/blob/8fadd2201f52936d998c66c47b0a0d37476c2ce5/Mafia/Models/ModelMorph.swift#L14-L119) provided leads for visual-type-3 loading, vertex links and per-LOD payloads. Independent REA/original-code reconstruction now loads the actual Tommy morph/Target model and renders bounded skeletal pose samples; eight-slot controller ownership and sequential playback with one live unit-weight slot are now implemented in bounded contracts. Full resource lifetime, weighted multiple slots and event integration remain open.

Several presentation choices cannot establish original behavior. Missing parents fall back to the root, only the first LOD is rendered, and some recognized visual types have no payload implementation. Material animation and lightmaps are skipped. [Scene lighting](https://github.com/alex-alex/LS3D-SceneKit/blob/8fadd2201f52936d998c66c47b0a0d37476c2ce5/Mafia/Scene/Scene.swift#L5027-L5062) uses tuned intensity scales. Our current unlit renderer also remains approximate; original driver, material, sector and visibility contracts are still required.

A specific incompatibility is [5DS interpolation](https://github.com/alex-alex/LS3D-SceneKit/blob/8fadd2201f52936d998c66c47b0a0d37476c2ce5/Mafia/Animations/AnimationLoader.swift#L387-L420): SceneKit uses smoothstep, while our original-code comparisons establish linear position interpolation in the supported domain. Keep that verified path. Bounded original rotation evaluation and sequential playback are now implemented; scale, notes, TCK root motion and audio-linked facial timing still require further recovery. No Apple application execution or SceneKit dependency was introduced by this audit.

## LS3D_FaceAnim

The complete two-file repository and both commits were inspected. Its [format description](https://github.com/kirill-mapper/LS3D_FaceAnim/blob/a25706b29970887c6d0ef2ab34f22c262f9d6ebd/README.md#L37-L58) and validator describe two header words followed by eight-byte records: six parameters and two trailing bytes. Import discards the trailing bytes; export writes `1,1` and canonicalizes the second header word. A future Rust reader should preserve all raw fields and validate lengths rather than assume those bytes are irrelevant.

The [weight equations](https://github.com/kirill-mapper/LS3D_FaceAnim/blob/a25706b29970887c6d0ef2ab34f22c262f9d6ebd/LS3D_FaceAnim.ms#L436-L503) map six bytes into ten weights across three regions. Each region sums to one; the editor multiplies weights by 100 for Max Morpher. This is a useful test reference, but native operation order and rounding remain unverified. The tool supplies neither joint/topology loading nor model compatibility checks beyond existing morph channels.

Mafia's 25 FPS and H&D2's 50 FPS are manual editor settings, not an encoded timing contract. New controllers default to Bezier; optional Step changes interpolation. Retained PC REA identifies candidate file/audio/state processing with 40 ms timing, but incomplete float recovery still needs instruction analysis and original-call probes. No Max execution, original facial playback comparison or H&D2/PS2 compatibility was established.

## mafia-re

This section retains the earlier pin and 2026-10-23 observations; the current
`109954d6` source refresh is recorded above.

The historical pinned [LS3DF card](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/docs/progress/LS3DF.svg) reports 97.60%, 1,350/1,361 source-associated functions and 98.39% average accuracy. The [current card](https://github.com/Marvisak/mafia-re/blob/0c44be32dd152c9722bbd8c2c8e18f112a279c04/docs/progress/LS3DF.svg) reports **98.46%**, the same **1,350/1,361** and **99.27% average accuracy**. This is improved instruction matching, with no increase in source-associated function count; average accuracy is not the percentage of exactly matching functions. These are upstream comparison results, not a playable-game percentage or Rust validation.

Game.exe now has a build target and one [WinMain implementation](https://github.com/Marvisak/mafia-re/blob/0c44be32dd152c9722bbd8c2c8e18f112a279c04/game/WinMain.cpp); its current card reports 1/5,037, 0.02% progress and 100% accuracy for that single function. The [main body](https://github.com/Marvisak/mafia-re/blob/0c44be32dd152c9722bbd8c2c8e18f112a279c04/game/Main.cpp) remains an explicit stub returning zero. No original menu or mission implementation follows from the bootstrap. A separate LS3DF 1.1 target reports 1,333/1,355, 95.12% progress and 96.69% average accuracy; it shares source with version macros and is not additive to the 1.2 count. Setup gains original-release and Steam language variants. These committed reports were inspected, not regenerated.

The [progress generator](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/tools/progress.py#L97-L143) uses reccmp's **effective per-function score**, not a byte count or runtime test rate. In pinned reccmp 0.1.7, an [effective match scores 1](https://github.com/isledecomp/reccmp/blob/2f6d84fc587efd1ff6453a17696a4951bcd09bd9/reccmp/compare/report.py#L53-L60); otherwise it contributes its raw comparison score. The [aggregate](https://github.com/isledecomp/reccmp/blob/2f6d84fc587efd1ff6453a17696a4951bcd09bd9/reccmp/compare/report.py#L160-L176) excludes stubs, with library/ignored entities filtered by mafia-re. If `M` is the eligible matched count, `S` its summed effective score and `D` the larger configured/known count, the bar is `S/D`, implemented is `M/D`, and accuracy is `S/M`. Functions have equal weight regardless of size or gameplay importance. Keep our Rust implementation and native-validation metrics separate; no upstream cards were regenerated here.

The [configured LS3DF and rw_data hashes](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/reccmp-project.yml#L1-L34) match our supplied binaries; Setup targets differ. This makes source addresses useful research leads, not new verification of each body. Written implementations retain nonmatching caveats, including [LitObject](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/I3d_lit_object.cpp#L1-L18) and [Joint matrix evaluation](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/I3D.h#L2870-L2902).

### Concrete engine entry points

Addresses below are **upstream PC virtual-address leads**, not newly admitted native function boundaries. LS3DF's preferred image base is `0x10000000`; subtract it for an RVA. Names retain upstream provenance until independently checked against symbols/instructions.

| Dependency | Pinned implementation and address lead | Useful next Rust work |
| --- | --- | --- |
| Provider and stream lifetime | [`dtaOpen`, rw_data VA `0x100018a0`](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/rw_data/rw_data.cpp#L221-L247) | Selected Installation archive open/read/close now uses the native facade. Recover remaining real callers and ownership, especially production texture-worker/surface effects; preserve exact selected names and fail-stop errors. |
| Concrete resources and geometry | [`LoadImage` / `LoadMeshImage`, LS3DF VA `0x10060cb0` / `0x10061120`](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/load4ds.cpp#L42-L286), [driver factories](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/I3d_driver.cpp#L1280-L1457) | Frame/visual construction, shared meshes, all admitted LODs, material references, registration and failed-load state. |
| Persistent model/scene lifetime | [Model `Open`, VA `0x100335a0`](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/I3d_model.cpp#L26-L89), [scene `Open`, VA `0x1004a300`](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/I3d_scene.cpp#L509-L563), [`LoadHierarchy`, VA `0x100603e0`](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/Load.cpp#L67-L190) | Real resource backends, reopen/close, derived destruction, hierarchy ownership, callback and failure ordering. |
| Material resources and execution | [`I3D_material::LoadImage`, VA `0x1002bd50`; animated loader `0x1002c000`](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/I3d_material.cpp#L409-L613) | Color-key/separate-alpha variants, texture sequencing, mipmaps and original material-to-draw state. |
| Coordinate space and lit draws | [`I3D_object::SetSpace`, VA `0x100378b0`](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/I3d_object.cpp#L437-L491), [LitObject `m_AddFGroups`, VA `0x10027880`](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/I3d_lit_object.cpp#L1501-L1604) | Shared-mesh detachment, all-LOD transforms, bounds, sector/lighting caches and buffer lifetime. Retain current unsupported-state rejection until verified. |
| Visibility and scheduling | [`ProcessPortals`, VA `0x10045700`](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/I3d_portal.cpp#L1734-L1778), [`Render`, VA `0x10062030`](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/render.cpp#L449-L484) | Sector membership, camera-driven portals/occluders, LOD and draw ordering; preserve sound/update dependencies. |
| Geometry-backed queries | [`TestColGeo`, VA `0x1000dec0`](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/i3d_collision.cpp#L795-L901), [`TestColSphere`, VA `0x10011d60`](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/i3d_collision.cpp#L1312-L1397) | Hierarchy/bounds traversal, filters, callbacks and nearest-hit state. KLZ loading and game collision remain separate dependencies. |
| Animation resources and remaining evaluation | [`I3D_animation_set::Open`, VA `0x10004900`](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/I3d_animation.cpp#L60-L106), [`I3D_interpolator::Update`, VA `0x1001d320`](https://github.com/Marvisak/mafia-re/blob/86a7b15cbfa002bc6222e5542671a81988472882/ls3df/I3D/I3d_interpolator.cpp#L113-L320) | Extend existing one-slot playback with scale, ordered weighted slots, pose/notes and real resource/event lifetime. |

The existing audited LS3DF union contains 764/1,255 pinned upstream candidate addresses as exact entries; 491 still require correspondence. These correspondence counts remain historical against `86a7b15c`; the current source refresh does not recount them. PC `GetVertexJoint` at RVA `0x54dd0` already has 2,040 admitted native comparisons. These retained address/case counts are discovery and validation evidence, not semantic coverage of every caller or new credit from this audit.

## MafiaUnity and DataFiles.md

The requested [DataFiles.md](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Docs/DataFiles.md#L1-L37) explicitly describes work in progress. It is a useful dependency inventory, not proof that every listed format runs. The audited develop commit dates to 2020-06-11.

### Data loading versus connected runtime

[`MissionManager.LoadMission`](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Core/Managers/MissionManager.cs#L33-L88) creates a root and reference maps, loads `scene.4ds` → optional `cache.bin` → `scene2.bin` → optional `tree.klz`, applies `MissionHacks`, then invokes its loaded callback. This exposes useful relationships between resources. **It is Unity's construction order, not recovered Game.exe boot order.** Its unload path also highlights the need for explicit mission/resource lifetime.

| Data | Actual inspected implementation | Reuse decision relative to current Rust |
| --- | --- | --- |
| `scene.4ds`, `scene2.bin` | Model hierarchy, named scene objects, patches, partial physical/door/light metadata | Existing Rust readers already handle bounded records and original script text. Extend generic named-reference/actor ownership and missing verified metadata rather than replace parsers. |
| `cache.bin` | Loads groups/instances; bounds remain opaque and only the first scale is applied | Rust already retains bounds, flags and secondary scale. Recover live residency/culling and shared instances. |
| `tree.klz` | Six shape arrays and links; spatial grid explicitly TODO | Valuable missing-format lead, followed by native grid/query/material semantics. No faithful collision implementation supplied. |
| `check.bin` | Standalone point/link reader; no mission loader or pathfinding caller found | Candidate graph intake, not AI/traffic. Preserve unknown fields until native interpretation. |
| `road.bin`, `car_table.bin`, `clusters.seg`, `effects.bin` | Listed in documentation; no parser or mission caller found in this tree | Original-binary research still required. |
| TCK | Parser exists; [player call is commented out](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Rendering/ModelAnimationPlayer.cs#L80-L100) | No root-motion runtime credit. |

The [KLZ array reads](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/API/MafiaFormats/KLZLoader.cs#L291-L342) give concrete layout leads, including an array order different from the header count order. But [face/cylinder conversion](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Internal/Generators/CityGenerator.cs#L69-L144) substitutes entire render mesh colliders, discarding triangle selection or cylinder geometry and surface properties. Preserve actual collision shapes and links when reconstructing Rust; visual meshes alone do not establish walkable surfaces or vehicle clearance. [CheckBIN's reader](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/API/MafiaFormats/CheckBIN.cs#L59-L113) suggests 30-byte points and 8-byte links, but neither graph directionality nor the unknown link float's meaning is established.

The [filesystem](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Core/FileSystem.cs#L196-L235) supports mod/loose/DTA lookup, but its automatic archive-mount call is commented out. Its precedence and duplicate handling differ from our bounded native-backed provider path; do not adopt them as original policy. DTA decompression and cache/scene/5DS/BMP/TGA intake already exist in Rust within explicit limits.

### Materials and presentation

The [face-group generator](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Internal/Generators/ModelGenerator.cs#L329-L469) is a useful checklist for keyed/transparent materials and texture sequences. Our renderer currently samples an opaque, unlit texture although the asset layer retains richer material metadata. The useful next abstraction is an original-backed draw/material descriptor with separate texture slots, alpha interpretation, sidedness, depth/sort state and cache identity.

Do not copy Unity's constants or shortcuts as native semantics: its [texture cache](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Internal/Generators/ModelGenerator.cs#L484-L545) keys only by path despite differing alpha decode options; the generator assigns a separate alpha map over diffuse `_MainTex`; its [texture player](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Rendering/TextureAnimationPlayer.cs#L9-L39) advances at most one frame and discards overshoot. These are source-level risks, not reproduced runtime failures in this audit.

Lightmap enum/shader declarations have no connected resource-loading path, and parsed portals have no visibility consumer in this tree. Sector [fog/ambient selection](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Utils/CameraExtensions.cs#L8-L138) uses AABBs, hierarchy and tuned multipliers. Keep native sector/lighting research separate. Likewise, the [animation player](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Rendering/ModelAnimationPlayer.cs#L278-L450) advances key-array indices at 25 Hz without using stored sparse timestamps for sampling. Our original-backed interpolation, controller ownership and skin arithmetic should remain authoritative within their admitted domains.

### Gameplay boundary

The [base startup](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Game/GameMain.cs#L57-L75) explicitly leaves the original menu TODO. [C# mod compilation](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Core/Modding/Mod.cs#L48-L95) executes `ScriptMain`; it is not Mafia's original mission VM. The [object factory path](https://github.com/MafiaHub/MafiaUnity/blob/0d7a12f03ce4385173377c3e9f457dcc2e891f24/Assets/Scripts/Game/ObjectDefinition.cs#L107-L150) installs Unity Rigidbody/capsule and idle character controllers, with no car special-type branch. No connected original campaign, traffic, police or mission-objective runtime was found.

Useful architecture leads are the mission lifetime boundary, staged named-object resolution, actor dispatch and data → interaction → visual/audio dependencies. Original ordering, physics, state transitions and constants must still come from REA and original execution. Keep our existing bounded script control-flow/scheduler contracts and bind their missing world effects; replacing them with C# mods would not advance original mission compatibility.

## Next general-engine work

The next bounded milestone is **the real file-backed IGraph → ITexture path
needed by the Scene constructor**, followed by its shared weather ownership.
Whole managed-texture conversion is a separate dependency. The current
`TextureResources::create_texture` creates blank constructor state;
`install_texture` and `register_cache` explicitly admit host state. They do not
perform filename lookup, image creation or factory cache acquisition.

1. **Immediate texture creation and identity.** Recover the filename/cache
   factory and reached flags-2 Create path using REA, actual PS2 naming evidence
   and original-instruction comparisons. Reuse the accepted ITexture/string/
   Close/Release ownership with real storage effects. Check source BPP and alpha,
   cache hit/miss and name/flag distinctions, returned references, failure output
   and actual graphics/pixel/string cleanup. Use synthetic format fixtures and
   original images as regression inputs, without a Snow-specific algorithm.
   A bounded canonical owner can precede conversion of other model textures;
   it must not claim that their separate CPU/managed lifetimes are now unified.
2. **Scene construction and shared teardown.** Bind viewport dimensions,
   weather reset, conditional shared Snow creation and sound defaults in the
   recovered order. Two admitted scenes must share one Snow identity: removing
   the first preserves it; the native last-scene condition releases the shared
   reference, while an external retained reference survives. Close does not
   own Snow release. Preserve the original unchecked Create result rather than
   inventing constructor rollback. Driver registration, optional listener,
   remaining Close effects and complete destructor/Open/reload each require
   their actual owners before receiving completion credit. Existing VB → IB
   cleanup remains bound; unavailable effects must still fail explicitly.
3. **Original mission ownership and scheduling.** Use the new `main` source
   map to research enclosing Mission Open/Close, Game initialization, actor
   factories/registration and the original player/camera loop. Then bind the
   existing script scheduler/world effects, replay and objectives. MafiaUnity
   and OpenMF provide name/patch regression ideas, not an original actor order.
   The authored MISE01 program probe remains at FINDACTOR/UnsupportedWorldBackend;
   application mission scripting is still unavailable.

Repeated resource create/release and scene construction/destruction must return
real allocations to the admitted baseline, preserving external survivors and
native failure prefixes. Each code milestone still needs binary-keyed contracts,
independent Rust implementation, appropriate original execution evidence and
two sequential independent reviews. Material/sector/portal rendering and
foreign dynamic actors remain parallel research dependencies. This source audit
changes no coverage count, graphical progress bar or first-mission gate.

## Direct PS2 symbol crosscheck

The supplied USA `SLUS_206.71` executable (SHA-256 `adf4944c5cdac23fcc4e9af9ef3ec1ba6234497e57cec2af2ff1a7a15c62a46f`) contains original function/global linker symbols, but no DWARF or STABS type/member information. mafia-re also distinguishes original symbols from guessed field names. The earlier batch checked 27 complete symbol ranges. The earlier singlemesh batch captured 22 related ranges plus two supplemental ranges; the captures overlap earlier work and are not an additive unique-function count. All are static evidence against unchanged original bytes, without PS2 execution credit.

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

Fresh static research associates the original PS2 `I3D_mesh_object::UpdateBoundVolume` symbol (VA `0x43c750`) with PC RVA `0x31400`. PS2 uses three LOD slots versus ten on PC, ref/count/slots offsets `0x30/0x34/0x38` versus `0x04/0x08/0x0c`, and VU/EE arithmetic rather than PC x87. These are original-symbol and instruction findings only, without PS2 runtime equivalence.

Fresh original PS2 symbols identify the IB/VB manager methods, with separately researched PC associations. PS2 manager allocation creates whole buffers, whereas PC pools shared blocks; PS2 VB allocation also lacks the PC FVF argument. Both PS2 `UnlockAll` bodies (VAs `0x479870` and `0x479ba0`) contain only `jr ra; nop`, unlike the PC lock-list cleanup. PC per-buffer shared-block allocation/free names remain descriptive research names, without matching original PS2 symbols. These instruction findings are static only, with no PS2 runtime equivalence.

Fresh static object research confirms a separate PC ten-generated-LOD cleanup layer. The original PS2 object teardown is split between `__dt__10I3D_objectFv` (VA `0x4243f0`) and `BogusDestructor__10I3D_objectFv` (`0x43f9c0`); neither captured body performs that PC loop. PS2 `SetMesh` (`0x43fa00`) lacks the PC keep-compatible/delete-ten path. The plain `Lod` name comes from PC diagnostic bytes, without a corresponding PS2 linker-name claim. These are static platform differences, with no PS2 execution evidence.

Fresh static texture analysis maps actual PS2 `ITexture` constructor/destructor, `Release`, `Close` and `Unlock` symbols to independently researched PC entries; `C_str` assignment also has an original PS2 symbol. PS2 uses a different texture layout and deleting-destructor convention. Its `Close` frees raw filename allocations, whereas PC assigns reference-counted empty `C_str` representations; surface Unlock uses vtable offset `0x14` on PS2 versus `0x28` on PC. These observations add no PS2 runtime validation. Representation-helper names remain descriptive/source-assisted, and only `m_RefCount` is supported as an original texture member spelling by the PC diagnostic.

The managed-removal helper at PC RVA `0x7aee0` retains a descriptive upstream name; no original PS2 counterpart is established. Its finite record/queue/pool cleanup and Close composition do not reconstruct creation, worker scheduling or pool shutdown. The existing PS2 caller difference is retained static context, without new PS2 execution evidence.

The creation helper at PC RVA `0x7ad20` is named `AddTextureToTextureManager` by its original PC diagnostic bytes. This supports the function spelling, without establishing a linker signature, original record/member/global names or a PS2 counterpart. Bounded record/filename/registry ownership is distinct from image loading or worker execution.

The producer at PC RVA `0x7a910` uses the pinned reference working name `ITexture::Request`; no original symbol or PS2 correspondence is established. Fresh REA/Ghidra confirms its finite producer body and data-table reference, without a recovered direct caller. Queue production does not load or upload textures.

The standalone pool-unload helper at PC RVA `0x7afe0` uses the pinned reference working name `UnloadManagedTextures`, without a recovered original PC/PS2 symbol. Fresh actual REA confirms its finite body and sole resolved direct caller; the call precedes the caller's scene-argument load. This does not establish whole SceneClose integration. REA reports the pool datatype as `undefined4`; pool offsets remain assembly evidence, without a recovered struct claim.

Bounded standalone managed texture Restore now reconstructs admitted system/default pools and reserves the registry through controlled device/storage effects. It composes the accepted Release dependency, selects raw format once, uses returned descriptor Size with signed budgets and wrapping memory arithmetic, grows arrays in 1,024-slot steps and preserves registry entries. RestoreManagedTextures is a pinned mafia-re working name without recovered original PC/PS2 symbol. Admitted plain-object loaded resources now connect to CPU ModelArena ownership. Full native texture Create/worker/upload, real GPU/device implementation, whole-device shutdown, derived lit/skin/morph resources and application/mission integration remain open; these standalone resource contracts establish no original-renderer parity.

The bounded pool-release helper at PC RVA `0x7a610` uses the pinned reference working name `ReleaseManagedTextures`, without recovered original PC/PS2 symbol. Fresh REA confirms per-pool system/slot release and array/header/memory cleanup after the locked unload prefix. Controlled native probes validate admitted state with seeded Rust intake; they do not establish real Direct3D destruction, Restore or whole-device shutdown.

Current upstream adds `tools/ps2fn.py`, which reads actual ELF symbol names and sizes and resolves disassembly call targets. Its generic Capstone MIPS decoder leaves unsupported R5900 instructions as raw bytes; this tool supplies neither member/type layouts nor new platform-equivalence evidence. The PC managed-texture section explicitly identifies its manager names as guessed and the subsystem as PC-only. `RenderThreadProc` at PC RVA `0x7a9b0` remains a working source name with an explicit instruction-matching caveat, without an established PS2 counterpart.

REA currently rejects the Emotion Engine R5900 variant with `architecture_unsupported`. The PS2 crosscheck used the retained dedicated EE Ghidra backend in read-only mode; PC REA and original-code comparisons remain separate. Future contracts record native binary/address, exact symbol, name origin, mapping evidence, Rust symbol and validation independently. This crosscheck adds no completed-function credit.

The native Open follow-up uses fresh REA evidence for eight bounded canonical entries and PS2 symbols for original names. Ghidra did not independently decompile UnlockPack at RVA `0x13b0` or Release at `0x1850`; accepted raw instruction boundaries and independent final-source reviews cover those limits. Static inventory counts are unchanged. Native stored-fixture comparisons, scripted Rust effect cases and the supplemental Rust-only installation probe remain distinct. The subsequent bounded WAV codec is implemented. Selected Installation menu/static scene archive routing is now connected; worker routing and app audio playback remain open.

The WAV milestone uses eight fresh PC REA results. Original PS2 linker symbols identify `WavDecompression` (VA `0x003ded50`) and `wav_table` (`0x0049ead0`); all 896 coefficients match the PC table exactly. This supports naming and numeric constants, without PS2 runtime or whole-platform semantic equivalence. Bounded PCM16 mono/stereo decoding has separate native archive/Read comparisons; generic non16 LZ, other channel configurations, Write and app audio playback integration remain open.
