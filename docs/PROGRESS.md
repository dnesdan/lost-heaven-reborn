# Reconstruction progress

Updated 2026-10-21 UTC. These measures describe separate stages of evidence. They do not estimate the percentage of the game reconstructed. Procedure counts depend on the binary and REA/Ghidra analysis profile; a procedure inventory is not a measure of understanding. Full-system verification requires traceable PC binary evidence and validation against original PC runtime behavior for every subsystem, with PS2 cross-checks where applicable.

## Current evidence snapshot

| Measure | Current result | What it means |
| --- | ---: | --- |
| Game.exe procedures in default analysis inventory | 4,914 | Procedures present in the selected analysis snapshot. |
| Game.exe procedures with retained pseudocode | 5 / 4,914 (0.10%) | Static decompiler output retained for unique procedures in that inventory; not a count of semantically understood functions. |
| LS3DF.dll procedures in default analysis inventory | 1,667 | Procedures in the default analysis profile. |
| LS3DF.dll procedures with retained pseudocode | 31 / 1,681 (1.84%) | Numerator uses an explicit 16-seed inventory profile that contains all 31 retained procedures. The default inventory has 1,667 procedures and is a different profile. |
| Manually instruction-validated finding total | Not measured | Existing observations are not normalized to a consistent finding count. See the subsystem evidence registry in [COVERAGE.md](../COVERAGE.md). |
| Original-runtime evidence | Tutorial input observed; no aggregate behavior count | The setup, main menu, tutorial load, forward movement/walk pose and a right-turn position change were observed. Timing and animation parity are unmeasured. |
| Fully system-verified subsystems | 0 / 12 (0%) | Strict evidence gate across the canonical subsystem registry in [COVERAGE.md](../COVERAGE.md); this is not an overall remake completion estimate. |

The original PC runtime was run through setup, main menu, and the Tutorial scene. Holding Up moved Tommy forward and showed a walk pose; holding Right changed his position. These are qualitative observations under Wine/QEMU and software rendering. Capture cadence does not measure the game's frame rate or physical movement speed. Driving, combat, mission completion, sound, and exact animation parity remain unverified. The PS2 symbol count is not a procedure count and is excluded.

## History

| Date | Revision | Milestone |
| --- | --- | --- |
| 2026-10-21 | [`69b73cb`](https://github.com/dnesdan/lost-heaven-reborn/commit/69b73cb) | Added a synthetic gameplay slice. |
| 2026-10-21 | [`cc07343`](https://github.com/dnesdan/lost-heaven-reborn/commit/cc07343) | Added traceable private 4DS conversion and subsystem coverage. |

Historical percentages are omitted because no comparable count snapshot was recorded for those revisions.

## Method and sources

Counts were taken 2026-10-21 UTC from retained JSON analysis records using REA 6.7.0 with Ghidra 12.1.4. The inventories include external/import entries (130 for Game.exe and 159 for LS3DF.dll). Game.exe SHA-256 is `08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27`; its default inventory profile ID is `7810ea9437da59780820ec6fedb47ce47390841a95d527ddb6c3b3700fe180cb`. LS3DF.dll SHA-256 is `e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6`; its default profile ID is `5fb8141f282b82b7a78df6b2d631f00eb93729605b53b333297ebfb4ff03fe9f`, and the explicit 16-seed profile ID is `46301268618f6f9f27a2856e15858155a42599f674def681aa6a56df9fe13c64`. The count record SHA-256 is `e72c38db3415595826a532bde14c989361a304c54f29e98d6fe8d1c2e9365642`. Game.exe uses its default inventory profile. LS3DF.dll's default profile contains 1,667 procedures; a separate explicit 16-seed profile contains 1,681 and is the denominator for its retained pseudocode numerator. All reported pseudocode addresses were verified as members of their stated denominator. Profiles must not be combined or compared as progress over time without a matching snapshot and profile.

These measures follow the general practice of separating binary/function progress and dated history shown by [mafia-re](https://github.com/Marvisak/mafia-re) and [AnyPS5](https://github.com/boykopovar/AnyPS5) ([history](https://boykopovar.github.io/AnyPS5/history.html)). This project uses its own evidence definitions and does not reuse their content.
