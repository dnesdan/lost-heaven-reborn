# Reviewed PR integration — 2026-10-29

The Rust integration source is `d8e9f3fc2ad92cb43052153149a001cd2b9a4206`.
PRs #51–57 preserve the existing resource owners and explicit unsupported
boundaries. They do not complete MissionOpen, GameInit, a script VM or the first
mission. `rust-engine` remains the sole integration/default branch.

| PR | Implemented scope |
| --- | --- |
| #51 | Retain sound incarnations across source I/O, publication and mission-cache reopen. |
| #52 | Reset navigation node/link flags on the existing owned graph, in source order. |
| #53 | Add compiler line-scan control flow with an explicit decoder/instruction-owner backend; only a synthetic backend is supplied. |
| #54 | Resolve installed navigation input into the existing loader, preserving optional absence, short-read prefixes and interrupted owners. |
| #55 | Specify ordered, single-use MissionOpen callbacks; production owners are not yet bound to them. |
| #56 | Admit world billboards through the existing derived-frame/resource path, including inherited mesh instances. |
| #57 | Prepare actual scene weather and Primary-sector lighting groups before scene2 intake. |

Each PR received two sequential independent Sol reviews. The combined tree and
manual resolutions also received two independent checks. Tests then exposed a
real integration defect: source billboard mesh instances used an entry limited
to Billboard duplication. The fix separates source mesh admission from that
bounded duplicate entry and preserves the destination Billboard class and its
orientation. The original failing assertions remain; a new regression releases
source frames before publishing the surviving shared mesh. The fix received
another two independent reviews. The duplicate entry's narrower host domain is
documented; native Duplicate supports additional classes.

## Executed validation

Linux x86_64, Rust 1.99.1, locked dependencies:

```sh
./scripts/test.sh
cargo test --workspace --all-features --locked -- --include-ignored --test-threads=1
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo build --locked -p lhr-engine
cargo build --locked -p lhr-engine --features engine-lab
```

The default gate passed **1,809 Rust and 106 Python tests**. The all-feature run
passed **1,833 Rust tests, including 21 software-Vulkan GPU tests, zero ignored**.
These counts overlap. Formatting, workspace checks and both Clippy variants
passed. Native Windows/Vulkan and macOS/Metal were not exercised.

A private harness linked the exact default gate-built libraries and loaded the
original MISE01 `check.bin`: 953,798 bytes, 13,857 nodes, 67,260 links, 13,860
reads and zero short reads. Inspection found no missing active records or
out-of-range node targets. Reset was idempotent and teardown left zero
allocations. Its tested reset bits were already clear; set-bit behavior is
covered by synthetic tests. These checks do not establish pathfinding.

The ordinary MISE01 load-session probe reached all 3,183 objects and 27 actors,
then stopped at actor kind 27 / byte 3,795,696 with exit 1. Terminal disposal
completed and every tracked owner was zero. Salieri world intake completed
276 source frames and 14 sectors, then scene2 stopped after 59 objects at byte
382,344 on `non-pristine cache hierarchy`. Its cache cleanup remained
interrupted with 44 frames retained until process exit. This is a newly
observed frontier, not a successful mission or cleanup gate.


## Native research and reference parity

The refreshed mafia-re reference is
[`dc867c3238488de27d094bb79b6550e392566e3c`](https://github.com/Marvisak/mafia-re/tree/dc867c3238488de27d094bb79b6550e392566e3c).
Two commits change 111 files. `C_game::Init`, `Done` and `DoneIntern` now have
substantial source bodies, still marked unmatched upstream. `GameLoopSingle`,
`SceneCallback`, mission Tick, `DecodeParams`, `Process`, road readers and
`C_entity` remain important missing dependencies. Source comparison does not
establish original-runtime equivalence or complete Rust parity.

Actual REA 6.7.0/Ghidra 12.1.4 analysis inspected the supplied PC `LS3DF.dll`
SHA-256 `e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6`:
driver-volume candidate RVA `0x7b8a0`, 172 bytes and 57 instructions. Default
analysis did not discover its procedure; a separate, recorded seeded profile
provided the focused result. Its evidence ID is
`ev_22b4082613ce2055a2b4f89a0d9d9f45f18da29c199e1ae89fe4015db8c3919e`.
The body stores driver volume and traverses live sound registrations in reverse,
marking sounds after an indirect query that the pinned source identifies as
logical playback. It does not directly change source volumes or multiply PCM.
Indirect call/caller coverage and runtime comparison
remain incomplete; the driver operation is not implemented by this milestone.

Fresh PS2 ELF metadata corroborates `SetVolume__11ISND_driverFf` at `0x4779c0`,
352 bytes, in SHA-256
`adf4944c5cdac23fcc4e9af9ef3ec1ba6234497e57cec2af2ff1a7a15c62a46f`.
This supplies naming evidence, not PC layout or PS2 semantic equivalence. The
REA/Ghidra route does not support this R5900 target. No PS2 emulation or original
executable ran. A separate PC instruction/jump-table check confirms weather ON
changes only bit 0 at scene offset `0x298`; it is not a fresh REA import.

Private binaries, original data, pseudocode, analysis outputs and raw logs stay
outside Git. No new original-function credit is awarded: 311 accepted scoped
entries / 114 contracts and the strict 0/12 system gate remain unchanged.

## Actual captures and repository cleanup

[Three fresh images and sanitized hashes](../screenshots/README.md#2026-10-29--reviewed-pr-integration)
use the tested executable SHA-256
`afad3901aa81d06371b7cd9017856a092b4e451d062f8631a97bd892ea27c329`.
The MISE01 headless/window modes exited 0 and actual Right input changed the
nonblack camera view. The taxi probe observed all five controls, moved
0.123774819 source units and exited 0 with zero tracked owners. It mixed
107,520 stereo frames at 48 kHz through a null sink; physical audibility is
unverified. Debug taxi initialization needed a 240-second window limit after
a 120-second attempt timed out.

Menu pixels were captured, but New Game stopped during INTRO on
`NumericDomain("Normalize")` at byte 39,797; interrupted cache teardown
produced exit 1. Black startup images and an invalid camera baseline were
excluded from accepted evidence. Final receipts confirm unchanged runtime
source/binary, reaped processes and removal of the owned temporary data alias.
No image generation or game-content replacement was used.

PRs #51–57 are merged and closed. Eight obsolete remote branches and the
merged local `work` branch were removed after ancestry/supersession checks
and a verified complete Git bundle. Its SHA-256 is
`ac38200f11e70da947e175bc6c6562817fab937cf0769031e081a44469d3a805`.
The superseded #50 branch and historical Godot stash remain recoverable
privately. Only `rust-engine` remains; history was not rewritten.
