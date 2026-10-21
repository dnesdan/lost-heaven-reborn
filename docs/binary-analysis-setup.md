# Private binary analysis tools

The Linux x86_64 workflow uses [REA](https://github.com/morluto/rea) with its
Ghidra provider. It performs static analysis without launching the selected
game binary. Keep original executables, imported programs, pseudocode,
snapshots and logs outside this checkout and public exports.

## Install

Prerequisites: Linux x86_64, Python 3.11+, Git, curl, tar, unzip, npm, and a
supported Node runtime: 22.19+, 24.11+, or 26+. Node 24.19.0 and npm 11.9.0
were used for the initial verified source build. REA's development toolchain
declares Node 24.18.0/npm 11.16.0; the installed runtime accepts the newer
Node 24.19.0. Dependency versions and integrity remain fixed by its lockfile.

```bash
scripts/setup_binary_analysis.sh
```

The script installs under `/workspace/lhr-tools` by default. Set
`LHR_TOOLS_DIR` to another external directory when necessary. It preserves an
existing REA checkout with a different revision or modified tracked files and
stops rather than overwriting it. Verified existing installations and archives
are reused; incomplete dependency/runtime outputs are rebuilt with `npm ci`
and the upstream `npm run build:cached` workflow.

| Component | Pinned identity |
|---|---|
| REA 6.7.0 | Git commit `6d356b84344659085f2b912857b5e27fe0160c9b` |
| Ghidra 12.1.4 PUBLIC | Official NSA archive `ghidra_12.1.4_PUBLIC_20260921.zip` |
| Full JDK | Eclipse Temurin 21.0.12.1+1, Linux x64 |

The Ghidra archive SHA-256 is
`ddac49f903da9d5bac833e5cc79395098b9c33cfd3279be5f31bd00387d2d4db`.
The JDK archive SHA-256 is
`ce79869e1307ed8ee1e2baa86a412b1eb5b75d10a01006d788a6f968bcfaee94`.
These values were checked against the official GitHub release asset digests;
the Ghidra release body and JDK checksum sidecar independently list the same
values. Downloads keep TLS and checksum verification enabled. The script
does not install another JRE globally, change agent registrations, or register
an MCP server in user configuration.

Setup checks the full JDK and Ghidra provider through REA's scoped doctor.
Health alone does not demonstrate a completed binary analysis.

## Analyze a private binary

Use absolute private input paths. The wrapper prints the path of a new private
JSON result and preserves the underlying REA exit status.

```bash
scripts/run_binary_analysis.sh analyze /absolute/private/program
scripts/run_binary_analysis.sh search /absolute/private/program 'a literal string'
scripts/run_binary_analysis.sh decompile /absolute/private/program 0x123456
scripts/run_binary_analysis.sh instructions /absolute/private/program 0x123456
scripts/run_binary_analysis.sh function /absolute/private/program 0x123456
scripts/run_binary_analysis.sh xrefs /absolute/private/program 0x123456
```

Default results are under `/workspace/lhr-private/analysis/cli`. Select another
external directory with `LHR_ANALYSIS_DIR`, or a new exact output filename with
`--output /absolute/private/result.json`. Files are created with owner-only
permissions and never overwrite existing files. The adjacent `.stderr.log`
retains diagnostics; `.ownership.json` records the input hash, process identity,
resource controls, exit status, and whether input bytes remained unchanged.
Failed or partial results remain private and must not be treated as findings.
PID and process-birth metadata are flushed while the command is running, then
updated at completion. Linux child subreaping keeps orphaned provider children
attributable if the CLI exits first. Cancellation and final cleanup use bounded
signals only after matching each owned descendant's recorded birth identity;
the record reports whether cleanup completed. A forced SIGKILL of the wrapper
cannot run cleanup, so its persisted identities are recovery evidence, not a
promise that an uninterruptible or forcibly killed task cleaned itself up.

Canonical paths reject inputs or outputs inside this checkout, including paths
that reach it through symlinks. Output directories are opened through directory
descriptors without following replacement symlinks. Temporary provider programs
also stay in an external `.rea-tmp` directory. The wrapper accepts a small set
of read-only commands; it does not forward arbitrary output or execution flags.

The wrapper caps the JVM heap at 6 GiB and inherits affinity to two CPUs chosen
from the current allowed set. `LHR_ANALYSIS_CPUS=2,3` can select a different
allowed set. REA independently caps its GC/compiler pools at two threads.
Import/analysis may take several minutes; the startup deadline is 30 minutes.
Heap size is not a complete process-family RSS limit. Task-owned child identity
and completion are recorded; cancellation first gives REA time to close its
provider, then applies bounded signals only to its verified owned descendants.

Each one-shot command creates an ephemeral Ghidra import and closes it afterward.
The setup tools and private result files survive; running processes must be
started again after restoring the cloud environment. Repeated commands can
repeat import cost. For many related queries, REA's documented stdio MCP server
supports a live session and private evidence/snapshot export. The official SDK
2 client takes its timeout as the second `callTool` argument; its default
60-second deadline can otherwise cancel a legitimate import.

The public wrapper uses the default PC analysis profile. It clears inherited
Ghidra seeds, language/compiler overrides and disabled-analyzer settings so a
profile selected for another platform cannot silently affect a PC baseline.
For deliberate seeded analysis or PS2 work, use a separate external service,
installation/profile and database, and record its exact configuration and digest.
Stock generic MIPS support does not establish correct PS2 R5900/EmotionEngine
custom instruction support.

## Evidence and validation

The workflow was exercised on private original PC executable and engine-module
inputs through real REA/Ghidra imports, function inventories, decompilation,
instruction queries and call graphs. The public wrapper is additionally checked
with an actual private analysis and synthetic path/symlink cases. Original bytes,
addresses, output files and detailed dossiers remain private.

Inventory size and successful decompilation counts measure static inspection
activity. They do not measure reconstructed gameplay or whole-game completion.
Label instruction observations, decompiler interpretation, hypotheses and
runtime verification separately. Full behavior claims also require original
runtime comparison and the recorded PC/PS2 crosscheck.
