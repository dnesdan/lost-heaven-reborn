# Progress

Updated 2026-10-21 UTC. The project has reset from a Godot gameplay prototype to a Rust/wgpu engine and game rewrite. The first Rust milestone reads a bounded set of loose original models and textures, renders a building and a crate, and supports camera inspection. Full campaign completion remains the goal.

| Measure | Current status |
| --- | --- |
| Rust workspace | Independent final run passed 13 Rust tests (7 assets, 3 engine, 3 renderer), 9 REA wrapper tests, formatting, workspace check and Clippy. |
| Asset reader | Five original building models decode to 747 vertices / 395 triangles. Loose 4DS v29 static meshes and supported BMPs only; DTA archive lookup is not implemented. |
| Linux renderer | Vulkan llvmpipe offscreen captures for original 4build101 (190 vertices / 98 triangles) and bedna00 (24 / 12) were read back and visually inspected. A focused XTEST Right-key input changed a bounded 2000-presented-frame camera-inspection capture; its three-frame window baseline matches the offscreen capture. |
| macOS / Metal | Toolchain selection is prepared, but no Mac validation has been performed. |
| Original gameplay | Not implemented. The inspection camera input is not original-game control or gameplay validation; rendered visuals have not been compared against the original engine. |
| Campaign | Not playable through the Rust engine. |

Historical private REA/Ghidra counts are 9/4,914 Game.exe procedures in the default inventory and 45/1,683 LS3DF.dll procedures in an explicitly documented union profile. Counts measure retained decompiler output only; they do not measure engine understanding. New bounded static REA evidence `PC-RUST-BOOTSTRAP-001` confirms selected boot, material, scene tick and bitmap findings, without establishing the full boot sequence or game loop. One private runtime trace measured 40 REA API units as 40 milliseconds on a specific unscaled path; do not generalize this to all clocks or animation paths. See [COVERAGE.md](../COVERAGE.md) for interpretation and boundaries.

## Reset boundary

The former Godot prototype, including its synthetic gameplay checks and private rendered captures, is historical. Its implementation and test results are not counted as Rust features or validation. Useful source-format, binary and original-runtime research remains available in private evidence. Public tests use synthetic inputs.

## Verification

Setup was repeated on Linux x86_64 with Rust 1.99.1 and the committed lockfile. The independent final `scripts/test.sh` run completed formatting, `cargo check --workspace --all-targets --locked`, workspace tests, Clippy, and all 9 REA wrapper tests. The bounded window path was exercised under Xvfb with a focused XTEST key input, and task-owned processes were stopped afterward. This is a development milestone, not release readiness. macOS/Metal remains untested.
