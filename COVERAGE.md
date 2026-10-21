# Reconstruction coverage

Status for the Rust rewrite as of 2026-10-21. This table tracks concrete capabilities and evidence; it is not a percentage estimate of game completion. Prior Godot prototype implementation and analysis are historical evidence only and are not counted as Rust functionality.

| Area | Rust implementation | Evidence and next work |
| --- | --- | --- |
| Workspace and application | Rust workspace and explicit renderer CLI are implemented. | Independent final run passed 13 Rust tests, 9 REA wrapper tests, formatting, workspace check and Clippy. A bounded Xvfb window path and XTEST input work for camera inspection; no original gameplay input is implemented. macOS/Metal remains untested. |
| Rendering and windowing | wgpu renderer draws indexed textured geometry with depth. Offscreen GPU readback and a bounded Xvfb window path both work on Linux. | Vulkan llvmpipe rendered and visually inspected 4build101 (190 vertices, 98 triangles, five material groups) and bedna00 (24 vertices, 12 triangles, one material group). A focused XTEST Right-key input changed the camera-inspection capture; the three-frame window baseline matches the offscreen capture. This verifies inspection camera input only, not game controls or gameplay. Rendering is approximate and unlit, with no original visual parity claim. macOS Metal remains untested. |
| Asset loading and world | Bounded loose-file 4DS v29 static mesh and 8-bit palette/24-bit BMP reader. Five supported original building models decode to 747 vertices and 395 triangles. | One original building and crate were rendered; four additional building types were decode-checked only. No DTA archive reader, full-city loader, original LOD selection, animation, skinning or gameplay integration. See [asset intake scope](docs/asset-intake.md). |
| Animation and characters | No Rust gameplay implementation established yet. | Historical PC static and runtime evidence is retained privately; implement and compare animation, movement and combat against the original. |
| Collision and vehicles | No Rust gameplay implementation established yet. | Recover and implement original collision, vehicle handling and interactions; compare with PC runtime. |
| AI and city simulation | No Rust implementation established yet. | Recover navigation, traffic, pedestrians and police behavior from PC evidence and cross-check PS2. |
| Missions and scripting | No Rust implementation established yet. | Recover the original scripting VM, mission logic, events and cutscenes; verify campaign progression. |
| Camera, audio, UI and saves | No Rust implementation established yet. | Recover and validate original controls, camera, sound, HUD, menus, localization and persistence. |
| Complete game | Not complete; no campaign is playable through the Rust engine. | Completion requires the full original campaign running on the Rust engine with the user's own game data and evidence-backed behavior. |

## Retained analysis

The private REA/Ghidra evidence records PC Game.exe pseudocode for 9 of 4,914 procedures in the default inventory profile, and 45 of 1,683 LS3DF.dll procedures in an explicitly documented union of profiles. These are historical analysis counts, not a measure of understanding or implementation. A new bounded REA/Ghidra contract, `PC-RUST-BOOTSTRAP-001`, confirms selected 4DS header/material and hierarchy behavior, scene-tick structure, and bitmap layout from LS3DF.dll. It does not establish full boot/main-loop behavior, render-pass equivalence, shader or lighting behavior, or PS2 parity. Original PC runtime observations and the narrower PS2 static cross-check are documented separately in private evidence. Keep hashes, addresses, confidence and limitations with those records; public summaries do not replace them.

The new dated private mapping `RUST-ENGINE-EVIDENCE-INDEX-20261021` connects this implementation, private render and XTEST captures, and the REA contract without modifying the earlier frozen progress indexes. Those counts remain 9/4,914 and 45/1,683; no new normalized counts are claimed.

Private animation runtime tracing observed 40 REA API units as 40 milliseconds on one unscaled path. This is a bounded observation, not a universal timing rule for the original engine. Validate each timing path against further static and runtime evidence before implementing it. The first Rust building capture is an approximate rendering proof only; it has no original-render comparison and does not establish visual fidelity.

Earlier Godot captures, parser tests and gameplay checks document the former prototype, not the current Rust application. They may inform future work but do not count toward Rust coverage. See [progress](docs/PROGRESS.md) for the reset boundary and current verification snapshot.
