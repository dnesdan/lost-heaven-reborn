Lost Heaven Reborn — Rust Engine Reconstruction

Rebuild Mafia: The City of Lost Heaven (2002) as a complete, standalone game engine written in Rust, based on reverse engineering of the original PC and PS2 binaries.

Repository: https://github.com/dnesdan/lost-heaven-reborn

This is not a remake with new gameplay. It is a faithful reimplementation of the original engine and complete game, preserving the original visuals, assets, physics, AI, missions, scripting, and behavior.

Technology

* Rust for the entire new engine and gameplay implementation.
* wgpu for rendering: Vulkan on Linux, Metal on macOS, with future iOS compatibility.
* Minimal supporting Rust crates for windowing, input, audio, mathematics, and serialization.
* No Unreal, Godot, Unity, SceneKit, or dependency on the original executable at runtime.
* Develop primarily in Linux Codex cloud, but maintain native Linux and macOS builds.

Reverse engineering

REA — Reverse Engineer Anything is the primary and mandatory reverse-engineering tool.

Install and configure REA with Ghidra on the available host. Use MCP or CLI to analyze the original PC executables, DLLs, and PS2 binaries where supported.

Reverse engineer the actual engine, not just its file formats. Recover and document:

* LS3D rendering, materials, lighting, world streaming, and visibility.
* File formats, assets, animations, skeletons, and collision structures.
* Vehicle physics, driving, damage, and traffic simulation.
* Character movement, weapons, combat, and AI.
* Police, navigation, pedestrians, and city simulation.
* Original scripting VM, mission logic, events, and cutscenes.
* Cameras, audio, HUD, menus, localization, and save/load.
* Interactions, timing, state machines, and dependencies between systems.

Analyze incrementally, starting with bootstrapping, asset loading, rendering, and the main game loop. Trace real functions, structures, constants, and call graphs. Compare PC and PS2 implementations where useful.

Do not guess when binary evidence is available. Record discoveries with binary hashes, function addresses, evidence, and confidence levels. Verify behavior against the original running game whenever possible. If REA cannot analyze a particular binary architecture, identify the limitation rather than fabricating results.

Implement the recovered behavior independently in idiomatic Rust. Do not publish copied proprietary source code or decompiler output.

Original game assets

Preserve the original Mafia graphics and content. Do not recreate or replace them unnecessarily.

The engine must read the user’s own legally obtained PC game installation. PS2 data may also be used for research and comparison.

Support original models, textures, animations, sounds, maps, scripts, and other required game data, converting or caching them locally when necessary.

Do not bundle copyrighted assets. The public repository must contain only independently written engine code, tools, tests using synthetic fixtures, and documentation.

Keep original binaries, game files, extracted assets, converted content, analysis databases, credentials, and private download URLs outside Git.

The final application should run using the user’s original game data, without requiring the original Game.exe or REA during gameplay.

Development workflow

Work autonomously and implement real functionality continuously. Prioritize a working engine over extensive planning.

Develop incrementally:

1. Initialize the Rust workspace, build system, and wgpu renderer.
2. Reverse engineer and implement original asset loading.
3. Render the actual Lost Heaven world using original models and textures.
4. Implement original animation, collision, movement, and vehicles.
5. Reconstruct the scripting VM and execute original mission scripts.
6. Implement AI, traffic, police, combat, cutscenes, and remaining systems.
7. Achieve full campaign compatibility and native Linux/macOS releases.

Each milestone must be tested and compared with the original game. Use automated tests, actual rendering, screenshots, and interactive gameplay verification where available. Clearly distinguish verified functionality from incomplete work.

Repository and agents

Use the existing public GitHub repository. Commit and push small, meaningful, tested changes regularly. Never force-push.

Use only Sol subagents for implementation, research and review; prefer Sol 6.1 where supported. The main agent coordinates integration and progress. Require two sequential independent code-review passes for substantial milestones, with fixes between reviews.

Maintain minimal English documentation:

* README.md — build, run, requirements, current status.
* AGENTS.md — development rules and architecture.
* PROMPT.md — this initial prompt.
* LICENSE — MIT for original project code.
* THIRD_PARTY_NOTICES.md — dependency licenses.

Communicate with me in Czech. Keep code and documentation in English.

Verification and cleanup

Build and test on Linux continuously. Maintain macOS/Metal compatibility and provide repeatable build commands. Verify actual rendered output and gameplay whenever graphical execution is available.

Clean up all task-owned emulator sessions, analysis tools, game processes, temporary mounts, and background jobs after testing. Preserve user files, reusable caches, and research results.

Never claim that functionality works without corresponding evidence.

The long-term completion criterion is the entire original Mafia being playable through the new Rust engine — not merely loading the city or demonstrating a few mechanics.

Start reverse engineering and implementing now. Do not stop after writing a plan.
