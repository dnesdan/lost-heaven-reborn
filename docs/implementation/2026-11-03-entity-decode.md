# Entity command decode (0x1000–0x103f) with execution refusal — 2026-11-03

Reference code: [mafia-re C_program.cpp](https://github.com/Marvisak/mafia-re/blob/d0fbd6dba9ac5b31cabd6661c9bb2b855eab5496/game/mission/C_program.cpp)
(entity DecodeParams/Process structure) with DecodeInstruction/DecodeParams/
Process/DecompileCommand bodies recovered from the shipped binary. Tested
against the local Game.exe SHA-256
08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27
(entity DecodeInstruction VA 0x51cdd0, DecodeParams 0x51b470, Process 0x51fda0,
DecompileCommand 0x51f3e0). Actual PS2 SLUS_206.71 strings corroborate the
command order and both spellings, not the PC table layout.

## Table reconstruction (capstone-verified, not decompiler-inferred)

- DecodeInstruction fills a stack array of `{name[32], id}` records at stride
  0x24 with the ID at +32: entry k spans `esp+0x18+k*0x24`, 121 entries scanned
  (limit 0x78). The shipped decompilers only render the first 13 entries, so
  the mapping was extracted directly from machine code instead: ID stores
  (`mov [esp+off], 0x100X`, direct and via `mov reg`) give exact entry→ID
  attribution, and copy-loop destinations (`mov esi, strVA` + `lea edi`) plus
  unrolled byte/dword stores give entry→string attribution. Desync artifacts
  (unaligned/duplicate stores) were filtered by the entry-stride congruence.
- Result: 64 command IDs 0x1000–0x103f. Most commands occupy an ENEMY_/ENTITY_
  pair sharing one ID; eight are singles with their own ID (COMPAREOWNERWITH
  0x1009, FINDCORNER 0x100a, BLUESCREEN 0x100f, TEXT 0x1010, ACTION_MOVE 0x1011,
  DEBUG_GRAPHS 0x1032, ACTION_CARJUMP 0x1033); NEVYHAZUJ 0x103d stores its
  single ENEMY_ spelling twice. VIDIM's internal format name is BLIND (0x101a).
- Cross-checks: all 64 IDs match the DecodeParams/Process switch cases;
  per-ID DecompileCommand formats match the DecodeParams arities (e.g. LOOKTO
  0x1003 one word, WAIT 0x1004 unchecked, USECAR 0x1008 zero-or-two words,
  VIDIM 0x101a zero-or-one word against `"0"`/`"<|>"`); PS2 rodata order
  agrees command by command.

## Implemented scope

- `ENTITY_INSTRUCTION_KEYWORDS` (121 entries, table order, both spellings)
  and `decode_entity_instruction`: entity table first, base 398 table fallback.
  The fallback is inferred: embedded entity programs mix base commands
  (LABEL/GOTO/WAIT/DIM_ACT/...) with entity commands, and entity DecodeParams
  explicitly delegates unknown IDs to base DecodeParams (GAME 0x461c70). The
  namespaces are disjoint — the closest pair, entity COMPAREOWNERWITH vs base
  COMPAREOWNERWITHEX, provably diverges before either terminator — so scan
  order is unobservable on admitted inputs. Base-only recognition is unchanged.
- `program_entity_parameters`: per-ID word-count admission for all 64 commands
  (every native `case` body's count checks, including unchecked accept-any
  arms, the 0x100c/0x1012/0x1022 at-least-two arms, and the 0x100d
  exactly-3/4/6/7 arm), over the shared comma-split tokenizer (16-word cap
  included, matching `local_4c[16]`). Rejection is purely count-driven in every
  native arm — keyword mismatches only select defaults — so admission is
  exact. Admitted raw words are retained (quotes/case preserved) for a future
  execution owner; values and keyword flags are not interpreted.
- Entity compile admission: `init_fresh_runtime` decodes Entity-vtable sources
  (entity table + base fallback, entity payload slot). Detector sources stay
  refused.
- Execution refusal: entity opcodes stop at the existing `UnsupportedCommand`
  dispatch arm, and `call_owned_subroutine` stays base-only. Entity payload
  disposal is host cleanup (decode runs no backend allocation boundary).

## Hard limits and documented deviations

- No Process: numeric conversions, keyword flags, NaN/flag defaults and the
  no-payload arms' uninitialized reads are not modeled. The one shipped line
  the original also rejects (`enemy_naprdelivaute` with zero words, 0x103a
  requires exactly one) is rejected identically.
- Two arms (0x1021 STEPAWAY, 0x1024 USE_DETECTOR) read word zero
  unconditionally; a missing word is fail-closed here because the original
  out-of-bounds read is not reproducible.
- ARM64 `ph_thing_ai` friction failure is pre-existing (untouched since
  PR #36, shares no code with this change). Independent review outstanding
  (no delegated reviewers available).

## Verification

```sh
cargo fmt --all -- --check
cargo test --locked -p lhr-engine --all-features --lib -- entity_ program_instructions:: program_compiler:: derived_program_source
cargo test --locked -p lhr-engine --all-features --lib
cargo clippy --locked --all-targets --all-features -- -D warnings
```

Table/predicate/compile/refusal tests (spot pair IDs, singles, order pins,
case-insensitive match, fallback IDs, all-64 count boundaries, census argument
shapes, mixed-table compile with stored opcodes/payloads, dispatch and
subroutine refusal, native-identical rejection). Full suite: 1806 passed, 11
ignored, 1 pre-existing ARM64-only `ph_thing_ai` failure. Real-data probe over
all 79 missions (3868 scripts): 9708 entity-table hits, 285 base fallbacks
(exactly ENEMY_CHANGEANIM/FORCESCRIPT/BLOCK/UNBLOCK/GETHOSTILESTATE, the five
base ENEMY_ opcodes), zero recognition misses, zero base misses (86461 base
hits), one rejection identical to the native one; 11 scripts skip on the
pre-existing unbalanced-quote parser limitation. MISE01 load probe frontier is
unchanged (kind 25, pumper): entity sources load undecoded and nothing at the
intake changed. No fresh GPU captures on macOS ARM64 (headless Metal mission
capture stalls without output); Linux software-Vulkan captures stay current.
