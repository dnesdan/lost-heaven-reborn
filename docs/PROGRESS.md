# Progress

Updated 2026-10-21 UTC. The Rust engine displays the Czech PC menu, static INTRO and a bounded MISE01 spawn neighborhood with the original static taxi. Phase 1 and phase 2 were accepted by two sequential independent reviewers. This is a static inspection flow, not mission execution or gameplay. Full campaign completion remains the goal.

| Measure | Current status |
| --- | --- |
| Rust workspace | `scripts/test.sh` passed 74 Rust tests (52 assets, 18 engine, 4 renderer) and 9 REA wrapper tests, including formatting, workspace check and Clippy. Two sequential independent phase 2 reviews accepted. |
| Asset reader | Reads the supported A8/A2/A6/A1/AA archive subset before loose files. Earlier private validation compared all 20,229 indexed resources (1,393,380,816 bytes) against a private prototype decoder, not an independent source extraction. Four known resources also match independent loose-source hashes. |
| Linux menu and renderer | Frozen X11/XTEST evidence verifies Down→Exit, Up→New Game, Enter/mouse→static INTRO, Escape→the MISE01 spawn neighborhood, orbit, repeat-load pixel equality, and Escape→menu. The crop selects 64 of 200 cache placements within 120 source units; 62 load and two validated billboard placements are skipped. The original static taxi is assembled across its 15 model roots. Original 4build101 and bedna00 archive captures match their prior loose-file PNG hashes. Scene rendering is approximate; 33 MENU, 12 INTRO and 62 MISE01 materials have effects omitted, and camera projection and lighting remain approximate. |
| Asset limits | Audio codecs and checksum verification are unsupported. Archive order follows static native startup registration for supported archives; selected-source failures do not fall back. |
| Rust menu | The Czech PC menu currently supports New Game and Exit. Unsupported actions are disabled and there is no profile chooser. New Game goes to static intro geometry; Escape switches to the MISE01 static spawn-neighborhood inspection and then back to the menu. This path does not run the original cinematic or mission. |
| Rust scenes and scripts | `inspect_scripts --control-probe` runs one bounded step per program using the literal LABEL/GOTO/GOSUB/RETURN/WAIT/COMMANDBLOCK/END subset in `PC-RUST-CONTROL-VM-01`. `PC-RUST-CONTROL-VM-RUNTIME-02` compares a command-block state transition and WAIT threshold against original runtime observations; the Rust harness matches those bounded states. GOTO/END comparison, the full scheduler/caller chain, replay, actor effects and mission parity remain unverified. Among the sampled scripts, 16 INTRO programs and 37 MISE01 programs halt on unsupported commands; one and seven respectively advance a prefix first, while two MISE01 programs are empty and complete. Prefix counts overlap halted counts. |
| macOS / Metal | Apple arm64 `cargo check --workspace --all-targets --target aarch64-apple-darwin --locked` passes from cache on the frozen phase 2 source. This is compilation only; macOS linking and Metal runtime are untested. |
| Campaign | Not playable through the Rust engine. |

Historical private REA/Ghidra counts are 9/4,914 Game.exe procedures in the default inventory (0.18%) and 45/1,683 LS3DF.dll procedures (2.67%). These are retained static pseudocode counts, not a measure of understanding or implementation. The strict evidence gate remains 0/12 systems; no overall remake-completion percentage is defined. Static contracts `PC-RUST-BOOTSTRAP-001`, `PC-RUST-ARCHIVE-001`, `PC-RUST-MENU-001` and `PC-RUST-CONTROL-VM-01` do not establish the full game loop. `PC-RUST-SCENE-001` records bounded parser input/output only. The separate `PC-RUST-CONTROL-VM-RUNTIME-02` trace compares bounded command-block and WAIT states; it does not establish the complete scheduler or caller chain. Frozen Rust menu evidence is in private manifest `rust-menu-validation/evidence.json`; original-game reference play is recorded separately. One other private runtime trace measured 40 animation API time units as 40 milliseconds on a specific unscaled path, not a universal clock rule.

## Reset boundary

The former Godot prototype, including its synthetic gameplay checks and private renders, is historical. Its implementation and tests are not counted as Rust features or validation. The original game's menu-to-prolog-to-first-mission runtime verification is reference-game evidence only. Public tests use synthetic fixtures; original game files and private analysis remain outside Git.

## History

| Revision | Milestone | Verification and analysis snapshot |
| --- | --- | --- |
| [`a077ef2`](https://github.com/dnesdan/lost-heaven-reborn/commit/a077ef25e887b6129c893daf9e5c4885904a2c31) | Started the standalone Rust engine and original loose-model renderer. | 13 Rust tests and 9 REA wrapper tests passed. Game.exe 9/4,914 (0.18%); LS3DF.dll 45/1,683 (2.67%); strict full-system evidence 0/12. |
| [`90d2edb`](https://github.com/dnesdan/lost-heaven-reborn/commit/90d2edbed08fa3b8a0227db9a3d45a00fe4b7664) | Added lazy reads from selected original DTA archives. | 27 Rust tests and 9 REA wrapper tests passed; two sequential independent reviews accepted. Private comparison covered 20,229 indexed resources against a private prototype decoder, not an independent extraction or original runtime. |
| [`d5e3754`](https://github.com/dnesdan/lost-heaven-reborn/commit/d5e3754b7c26a0b408f051a825114208de2a3b36) | Added the original Czech menu and bounded static INTRO/MISE01 inspection. | 61 Rust tests (49 assets, 8 engine, 4 renderer) and 9 REA wrapper tests passed; two sequential independent reviews accepted. Menu input and static-scene captures verify the temporary inspection flow, not cinematic or mission execution. |

## Environment and verification

Use `./scripts/setup.sh` to install the pinned Rust 1.99.1 toolchain into the shared tools directory; run `./scripts/test.sh` for formatting, locked workspace check/tests, Clippy and REA wrapper tests. This setup, full suite and X11 checks were exercised on Linux x86_64. Apple arm64 target compilation passed for the phase 2 baseline; native macOS linking/Metal remains untested.
