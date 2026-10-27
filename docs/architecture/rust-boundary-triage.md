# Local Rust boundary triage

Run from a complete checkout, using Python 3 and Git:

```sh
python3 scripts/rust_boundaries.py > /tmp/lhr-boundaries.json
python3 scripts/rust_boundaries.py --base BASE_COMMIT --check
python3 -m unittest discover -s tests -p 'test_rust_boundaries.py' -v
```

Replace `BASE_COMMIT` with the reviewed PR base. The tool resolves it once to a
commit and reports that SHA. It never changes the baseline, sources or Git refs.
No CI workflow or dependency is added. Its regression tests also run from the
existing `scripts/test.sh`; that script still stops on a failed Rust gate.

The inventory reads tracked and nonignored untracked `.rs` files under `crates`.
Comments and literal contents are excluded. Raw identifiers keep their symbol
identity: `r#unsafe` is not the unsafe keyword, but `r#asm!` is still an assembly
macro. Tests, inactive cfg branches and macro bodies remain included. Symlinks,
skip-worktree Rust files, invalid encoding, malformed comments/strings, missing
base history and empty source trees fail rather than yielding a clean audit.

Findings separate unsafe keywords, assembly macros, raw pointer types, selected
raw-memory symbols, selected compatibility API names and x86 register names.
Safe references, `Rc`/`Arc`, integer flags and provenance addresses are not counted
as raw-pointer debt. Classify each result manually as format, platform, numeric
compatibility, diagnostic/test or semantic engine code before proposing a change.

Exit codes: 0 means inventory completed (or no per-file token-count increase in
check mode); 1 means check mode found increases; 2 means the audit could not run.
This is deliberately a **lexical triage tool, not a Rust parser, semantic lint,
complete unsafe inventory or safety proof**. It cannot resolve imports, expand
macros, inspect dependencies or establish pointer soundness. Replacing an existing
use with another in the same file can preserve token counts and escape detection.
A zero result does not authorize a merge or claim parity. New justified boundary
uses require review; do not move the base forward merely to hide them.

See [runtime boundaries](rust-compatibility-boundaries.md) and the root
[development conventions](../../AGENTS.md) for the migration rules.
