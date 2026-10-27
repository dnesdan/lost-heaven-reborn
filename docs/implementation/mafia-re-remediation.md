# Current implementation and evidence — 2026-10-26

This is the retained 2026-10-26 report. See the
[2026-10-27 parity continuation](parity-continuation.md) for the current source,
actor frontier, material playback, three-review PR stack and validation limits.

Reviewed code: staged Git tree `28e0b087eab47c810aba6ee139962d9e0a9a0424`;
375-file source manifest `43af8105831f94e4df7eabf5d7e031031d203b28ef3e3dda1422b082e8a6823b`.
Two sequential independent applied-source reviews passed (`d6133b45…`,
`877ade88…`). The receipts and full hashes are recorded in [progress.json](../progress.json).

`scripts/test.sh` passed: **1,561 Rust tests and 56 Python tests**, formatting,
workspace checks, Clippy and generated progress. All **13 opt-in Vulkan tests**,
**48 Engine Lab binary tests** and all-feature/all-target Clippy also passed.
These are Rust source-profile checks; no original executable or function ran.
The standalone MISE01/menu and sky proofs below used the first reviewed
source snapshot (`0c60e6fc…`). Later changes bind shared driving audio and tag
its actual Scene/Vehicle frame owners. A final-source original-data replay
loads 132 scene sounds and 907 city models, completes a real 16 ms driving tick
and PCM mix, then closes with zero tracked audio/Scene owners.

| Path | Exercised behavior | Remaining boundary |
| --- | --- | --- |
| Scene rendering | [MISE01 road and sky](../screenshots/2026-10-26-mise01-road-sky.webp): 267 draws, including five `zapad` backdrop groups. Changing only those GPU textures to black changes 136,934 pixels; restoring them restores identical full-frame RGBA bytes. The scene then closes with zero frames, portals and sector allocations. | Native pixel parity, general GPU fog/environment effects and streaming. |
| Object intake and disposal | The explicit `Models` CPU probe loads all 3,183 MISE01 objects after 1,415 world frames. It stops at actor dispatch, cursor 3,794,014 / Game `0x544a16`. Before disposal: 10,570 frames, 423 visual textures, 37 PCM sources and 655 saved-lighting LODs. Genuine cache/Scene/registry disposal reaches zero in every reported owner category. | Actors, scripts, deferred sound links and complete SceneOpen/MissionOpen. |
| Materials and visuals | Selected-LOD skin/morph poses, Target links, bounds and cache copies use owned state. Billboard orientation covers all 12 axis/mode combinations; actual `tunelx`/`semafor` use axis 2/mode 0. Draw-matrix lighting, packed alpha and per-group raw-bit blend tests are connected. `watercity` retains 20 initial animated texture frames. | Texture-animation ticks, animated lit visual type 1, all-ordinal preframe/static cross-visual material-table order, and the native Lensflare GPU pass. |
| Shared resources | Dummy, Sound, AREA and fog-light owners compose with the same arena and resource lifetimes. Tests cover two simultaneous scenes, last references, ordered listener retirement and interrupted cleanup. Selected-LOD saved-lightmap VB/IB, UV2 and atlas leases are retained through failure and released by their owners. | General dynamic lighting and atlas resize/16-bit/offline profiles; AREA original-runtime comparison. |
| Menu | [Menu sky capture](../screenshots/2026-10-26-menu-sky.webp) renders its retained prefix. | `menugl` reaches unsupported model animation at cursor 466,049; the interrupted cache remains retained. Companion 5DS rotation/scale and authored menu startup remain open. |
| Game helpers | Fourteen PC-static entry witnesses support typed controls, a 64-bit key mask, camera reset, four definition buffers and ordered load/save dependencies. | Active HUD/profile backends, actor registration and original Game scheduling. These helpers do not establish gameplay wiring. |
| Driving and Lab | [Actual Lab window](../screenshots/2026-10-26-engine-lab.webp): W/A/D/S/Space and motion, FOV 50°→65°, stable pause counters, one 16 ms step, focus loss and cleared held controls. Three F4 reloads and Escape exit 0 produced four zero-owner cleanup reports. Resume clock-gap behavior has separate unit-test evidence. | Wide host chase view can be obstructed by geometry. Original camera/collision and authored mission progression remain open. |
| Platforms and reference | Linux software Vulkan was exercised. The implementation reference `f42a25d9`, fresh [mafia-re `b86ba567`](https://github.com/Marvisak/mafia-re/tree/b86ba567e404e211048fb2bf6eb1a4f133f0fd2f) delta audit, PC REA receipts and PS2 symbols are tracked separately. | Native Windows/Vulkan, macOS/Metal, hardware audio and original-runtime comparisons were not run. |

The [six mission gates](../GOALS.md) remain open. Accepted scoped-function
and system-gate totals are unchanged; see [coverage](../../COVERAGE.md).

[Run commands](../RUNNING.md) · [progress](../PROGRESS.md) ·
[reference audit](../REFERENCE_PROJECTS.md) ·
[preceding implementation page, verbatim](../history/2026-10-26-pre-final-documentation.md#docsimplementationmafia-re-remediationmd).
