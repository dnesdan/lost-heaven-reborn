# Stage 7d: data-language routing for the live menu

## Evidence and connected behavior

Reference: local `Marvisak/mafia-re` at
`29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c`. This is source-only work:
no new binary reconstruction, function/progress credit or original-runtime
comparison is claimed.

- `game/Main.cpp:763-787` selects the language from the first two bytes of
  `tables/version.txt`. A missing file or fewer than two bytes selects `en`.
  The original subtracts `0x20` for uppercase ASCII bytes. That unusual
  operation is preserved: `EN` produces the bytes `0x25, 0x2e`.
- `game/ui/g_loadsave.cpp::LanguageSelect` inserts those two bytes into
  `tables/textdb_xx.def`. `game/ui/g_indicators.cpp::LanguageSelect` inserts
  them into `tables/lang_xx.def` for ordinary nonzero language codes.
- `game/ui/g_textdb.cpp::LoadDatabase` and `g_textdb.h` retain zero-terminated
  byte strings addressed by file offsets. `RawTextDb` shares the existing
  bounded parser and keeps those bytes. The existing Windows-1250 Unicode
  inspection API remains available with its previous rejection policy.
- `game/ui/g_indicators.h` identifies the language table's font suffix and
  character lookup table. The existing 568-byte table layout supplies the
  selected map directly to the menu's byte-to-glyph renderer. This removes
  the previous Czech Windows-1250 Unicode round trip and does not infer a
  new code page for another installation.

The actual `MenuUi::load` used by application startup now follows this route
instead of hardcoding Czech text and character tables. The shared version-byte
helper also serves the existing `SystemState::init_system` language effect,
without changing its read-size validation or effect/close order. Menu labels,
font mapping, CPU text placement and menu actions consume the selected data.

`Installation::read_optional` returns absence only for an unregistered/missing
path. Archive selection, snapshot errors, unsafe paths, ambiguous loose names,
nonregular files and other IO failures remain errors. The existing `read` API
keeps its missing-asset error behavior. English is used only for a missing or
short version file; a broken selected source or absent selected text/map never
falls back to another language or provider.

## Admission and scope boundaries

The menu admits safe ASCII language filename bytes and a terminated safe
ASCII font suffix from the selected table. It reads that family's actual
`font1`/`font2` GFnt metrics and TGA atlases through the existing bounded
parsers; there is no font-family allowlist or substituted Latin default.
The selected map must have the expected `Lang` marker and two-byte code,
exact extent and terminated suffix. Missing/malformed family resources and
unsafe filenames fail explicitly. Font/image decoding and existing menu
quotas are unchanged.

The viewer reads bounded resource snapshots through its existing Installation
owner. This does not reproduce native partial-read traces, text allocation
identities or a full mutable `G_LoadSaveClass` owner. The `LanguageSelect(-1)`
mission/game flag branch, indicator-only zero code, unconditional Czech credits
table, credits display and profile/language UI remain separate dependencies.
The existing menu canvas and enabled action subset remain host presentation,
not a claim of original menu-controller or campaign equivalence.

## Synthetic validation

Tests load actual temporary loose-file installations with independent English
and Czech text/map files, real bounded menu/font/TGA fixtures, and a byte that
the existing Windows-1250 Unicode API rejects. They verify that the selected
byte labels and glyph widths reach CPU menu rendering and that actions remain
connected. Further checks cover missing/short versions, long-version prefixes,
the uppercase subtraction, malformed/missing selected text, nonregular version
files, mismatched map markers and unsafe/unterminated font suffixes. A second
synthetic font family has different glyph widths and must reach the real text
renderer; deleting its selected metrics must fail rather than select Latin.

The optional-read regression includes a selected archive alongside a loose
English copy, then corrupts the retained archive and requires an error rather
than fallback. Raw TextDB tests preserve bytes and the opaque header while
retaining offset, truncation and aggregate quota checks. No private game files
or original-runtime data are included.


Rust 1.99.1 verification used the fresh dedicated `target-stage7d-language`:

- `scripts/test.sh`: formatting and workspace/all-targets check passed. The
  engine library ran 1,053 passing tests and the same two established high-FIP
  x87 environment failures. Their implementations and tests were not changed.
- Full workspace tests with only those two known test names excluded: assets
  library 123 passed; native installation integration 8 passed; engine library
  1,053 passed; engine binary 29 passed; program integration 8 passed; renderer
  5 passed. Documentation test targets also passed.
- Workspace/all-targets Clippy with `-D warnings`: passed. Its test-only RGBA
  iterator correction was followed by another complete engine binary run,
  with all 29 tests passing.
- Python progress tests: 47 passed; generated progress check passed unchanged.
- Python binary-analysis boundary tests: 9 passed with host process visibility.

These are build, source-contract and CPU composition checks. No GPU render,
interactive original-data menu session or original-runtime comparison was run.
