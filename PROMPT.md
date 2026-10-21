Build **Lost Heaven Reborn**, a playable **Mafia: The City of Lost Heaven (2002)** remake in **Godot 4**, in the existing public repository:
[dnesdan/lost-heaven-reborn](https://github.com/dnesdan/lost-heaven-reborn)

Work autonomously in Linux Codex cloud. Deliver Linux first, with macOS/iOS support designed in from the start. Speak Czech with me; use English for code, commits, and documentation. PSP is outside scope.

## Repository

Inspect the checkout, branch, instructions, and existing work. Clone only if needed; initialize if empty. Preserve existing work and history; never force-push. Use the task branch and commit/push small, verified milestones through available Git/GitHub access.

Keep documentation short and practical: `README.md` for setup, commands, status, and next step; `AGENTS.md` for conventions, verification, and cleanup; `PROMPT.md` containing this prompt with private inputs redacted; `LICENSE` using MIT for our original code; `THIRD_PARTY_NOTICES.md` recording reused sources, revisions, licenses, and modifications. No filler or speculative documentation.

## Setup

Detect architecture, memory/disk limits, network policy, graphics, and available UI controls. Install missing dependencies from official sources as needed; pin working versions, cache downloads, and limit concurrency.

Install a stable Godot 4 release from [official releases](https://github.com/godotengine/godot-builds/releases) with matching export templates. Use **Mobile rendering: Vulkan on Linux, native Metal on supported Apple devices**. Start with GDScript; add C++ GDExtensions only for concrete needs. Keep importers portable and reuse existing parser languages.

Provide repeatable setup, import, run, test, capture, and export commands. Install [REA](https://github.com/morluto/rea), Ghidra, their documented Node/JDK dependencies, and required Linux utilities when needed. Use REA's CLI if MCP is unavailable; investigate specific unknowns and validate findings.

## Private inputs

I will provide `MAFIA_PC_URL`, `MAFIA_PS2_URL`, and optionally `PS2_BIOS_URL` through environment variables or ignored configuration; also accept local paths. Use resumable downloads, record SHA-256 hashes and game editions, and preserve originals read-only.

Use PC data/behavior as the primary reference. For additional evidence, use PS2 data and a compatible emulator: [PCSX2](https://pcsx2.net/downloads/) on supported Linux hosts, or [ARMSX2](https://github.com/ARMSX2/ARMSX2) on compatible hosts. Use isolated Wine prefixes for PC reference runs and separate emulator settings/saves. Check host and PS2 analysis support before investing in setup.

Exclude original/converted assets, media URLs, credentials, BIOS, decompiled game code, and analysis databases from Git and public artifacts. Audit staged files and outgoing commits; public CI uses synthetic fixtures.

## Existing work

Inspect relevant code and follow useful upstream references:

- [mafia-workbench](https://github.com/b4rtt/mafia-workbench) and [MafiaUnity](https://github.com/MafiaHub/MafiaUnity): parsers and runtime research.
- [Max4dsTools](https://github.com/pudingus/Max4dsTools) and [MafiaBlenderToolkit](https://github.com/Richard01CZ/MafiaBlenderToolkit): conversion.
- [mafia-re](https://github.com/Marvisak/mafia-re), [PC analysis](https://github.com/st0rm94/mafia-reverse-engineering-export), and [PS2 decompiled](https://github.com/st0rm94/mafia-ps2-decompiled): behavior references.
- [LS3D-SceneKit](https://github.com/alex-alex/LS3D-SceneKit), [bully-remake](https://github.com/oh-ashen-one/bully-remake), and [awesome-mafia](https://github.com/MafiaHub/awesome-mafia).

Copy/adapt license-compatible code with attribution. Use unlicensed projects as research references unless permission is obtained. Treat decompiled output as evidence requiring validation.

## Implement and verify

Separate importers, intermediate data, and gameplay. Convert suitable models/animations to GLB; preserve scene, collision, script, and gameplay metadata separately. Verify axes, units, materials, skeleton hierarchy/rest pose, skinning, and animation timing. Preserve the original style and available Czech text/audio.

Deliver progressively: original model; recognizable district with collision; Tommy with original animations and third-person camera; original car with enter/exit and driving; checkpoint task with HUD, failure, completion, and restart; packaged Linux game independent of the editor, emulator, and analysis tools.

Reproduce vehicle handling deliberately; verify steering, braking, collisions, and enter/exit. Default vehicle physics is only a prototype. Add traffic, police, and further missions after the slice works.

Actually operate and inspect the game: movement, camera, driving, and restart through real inputs; capture screenshots/video, inspect frames, fix defects, and compare against original-game reference captures. Use available desktop controls or isolated X11/Xvfb with xdotool. Keep captures private.

Prove a small Vulkan render first. Xvfb provides no GPU acceleration; software rendering is acceptable when verified and labelled. Headless checks do not prove visuals. Distinguish builds, logic tests, rendered checks, and interactive tests; report approximations and blocked checks honestly.

Prepare macOS/iOS export presets early. Use a Mac with Xcode for iOS export/signing and test Metal on Apple hardware, including a physical iPhone when available. Define shared input actions with touch/gamepad controls for walking, camera, interaction, and driving; check pause/resume. Linux rendering or simulator screenshots do not validate Metal. Missing Apple access must not block Linux progress; provide exact remaining verification steps.

## Cleanup

Track owned processes, sessions, mounts, and temporary settings in an ignored ownership record. On success, failure, or cancellation, close gracefully, wait for writes, then use bounded TERM/KILL only for verified task-owned processes. Never globally kill applications. Recover leftovers safely on the next run.

Restore temporary settings and remove only owned scratch files. Preserve inputs, saves, installed tools, reusable caches, imports, and useful evidence. Leave no owned background jobs running.

Start implementing now. Continue through playable milestones; finish independent work before asking one specific question about a genuine blocker.

## Orchestration and code review

Act only as the orchestrator: plan, assign tasks, track acceptance checks, coordinate integration, and handle Git. Do not implement code yourself. Delegate research, implementation, testing, fixes, and reviews to **Luna 6 (`gpt-6-luna`)** and **Sol 6.1 (`gpt-6.1-sol`)**. Use Luna for focused tasks and Sol for complex changes, integration work, and reviews. Give each task one owner, a clear scope, and concrete acceptance checks. Use separate worktrees or non-overlapping file ownership.

Require **two sequential, independent code-review rounds for every code milestone**:

1. A reviewer who did not author the code checks correctness, requirements, integration, and verification evidence. Delegate fixes, then rerun relevant checks.
2. A fresh reviewer examines the final revised diff, including all fixes, for remaining defects and regressions.

No agent may review its own code. Resolve material findings and independently recheck subsequent fixes before accepting, committing, and pushing the milestone.

If spawning or a requested model is unavailable, report the exact limitation; never claim those agents or reviews were used, silently substitute models, or implement the code yourself.


[Private input source footer redacted in full.]
