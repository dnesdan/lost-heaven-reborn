# Main integration — 2026-10-27

The user requested two code-review passes over every open PR, fixes, sequential
merging into `main`, a fresh mafia-re parity plan and a Rust architecture audit.

## Integrated scope

There were 14 open PRs. `main` did not exist, so it was created at the existing
`rust-engine` head `e58473152af3cb38e0eaa3b0e726f5de6823e5b2`. Branch histories
were preserved with merge commits; no force-push, branch deletion or CI workflow
was introduced. The repository's default-branch setting remains `rust-engine`;
use `main` explicitly for this integrated state.

| PR | Scope | Integration commit on main |
| --- | --- | --- |
| [27](https://github.com/dnesdan/lost-heaven-reborn/pull/27) | Sound cache and audio flags | `298a31f` |
| [28](https://github.com/dnesdan/lost-heaven-reborn/pull/28) | Fade, backdrop, alpha, depth bias | `a9b19ff` |
| [29](https://github.com/dnesdan/lost-heaven-reborn/pull/29) | Ordered actor/program-source intake | `694bef0` |
| [30](https://github.com/dnesdan/lost-heaven-reborn/pull/30) | Interrupted mission cleanup | `350c0bb` |
| [31](https://github.com/dnesdan/lost-heaven-reborn/pull/31) | Menu navigation | `7948c71` |
| [32](https://github.com/dnesdan/lost-heaven-reborn/pull/32) | Animated material owners | `0d1ee01` |
| [33](https://github.com/dnesdan/lost-heaven-reborn/pull/33) | Original mission directory names | `2e08b68` |
| [35](https://github.com/dnesdan/lost-heaven-reborn/pull/35) | MRPH ownership and deformation | `4cc31c1` |
| [36](https://github.com/dnesdan/lost-heaven-reborn/pull/36) | Startup source-patch lifetime | `aadde28` |
| [37](https://github.com/dnesdan/lost-heaven-reborn/pull/37) | Owned navigation dependency | `40411d6` |
| [38](https://github.com/dnesdan/lost-heaven-reborn/pull/38) | Engine Lab integration and new camera fix | `1415822` |
| [26](https://github.com/dnesdan/lost-heaven-reborn/pull/26) | Original workbench | Included through #38; GitHub confirmed merged |
| [39](https://github.com/dnesdan/lost-heaven-reborn/pull/39) | Raw byte frame names | `4c8c8fe` |
| [40](https://github.com/dnesdan/lost-heaven-reborn/pull/40) | Read-only loader diagnostics | `6e79027` |

#26 was retargeted before #38 was merged. Its actual head was already an ancestor
of #38, so it was integrated with the newer lifecycle/time corrections rather
than separately applying its older tree. GitHub subsequently reported all 14
merged, with no open PRs remaining before this documentation follow-up.

Integrated code head: `6e790279dd5d9bf2a111d0a9330527e8bc4b968a`.
The GitHub comparison from the former stack tip
`4ceb0e58a95c1221819c37c2797cdd7afb20c2aa` to that head is ahead-only, with exactly
one changed file: `lab_camera.rs`, 48 additions and one deletion. Thus the merge
sequence preserved the prior complete stack and added the reviewed camera fix.

## Two review passes and the fix

Each PR received two additional COMMENT review submissions anchored to its
reviewed head. They are **two source passes by the same assistant**, not two
independent reviewers. Delegated-agent execution was unavailable. The first pass
covered behavior and integration; the second revisited ownership, failure paths,
API boundaries and regression expectations. See each PR's review history.

One new confirmed correctness issue was fixed in
[`ed076f4`](https://github.com/dnesdan/lost-heaven-reborn/commit/ed076f4497a886e9e0ff1b79b64d0b74138ea759):
Engine Lab orbit used `clamp(near * 2, far * 0.8)`, although a valid camera only
requires `far > near`. For `near=1, far=2`, the bounds are reversed (`2, 1.6`),
which can panic. The correction validates the input first, keeps ordinary margins
when valid and otherwise uses the validated near/far interval. Invalid output
still cannot partially replace the camera.

Two Rust regression tests were added for tight ranges, normal margins and atomic
invalid-input handling. The final source change was re-read before merging.
No other newly confirmed blocking source finding remained in the reviewed PR
scopes. This is not a proof that the integrated repository has no defects.

## Verification: what ran and what did not

| Check | This integration |
| --- | --- |
| Connected terminal / agent host | Attempted; returned `tunnel_client_not_seen`. |
| Fallback environment | No Rust toolchain; could not retrieve the full checkout/toolchain. |
| Fresh Cargo, rustfmt, Clippy, `scripts/test.sh` | **Not executed.** |
| Original game-data probes, GPU checks, interactive captures | **Not executed.** |
| Camera bounds arithmetic | A separate Python float32 model checked 45 finite valid clipping pairs, including tight/overflow cases. This is not Rust or camera-runtime verification. |
| GitHub integration | Exact expected head SHA used on each explicit merge; #26 ancestry and final ahead-only comparison checked. |

The user explicitly requested merging with runtime verification where possible.
#39 and #40 were therefore moved out of draft for source integration, not because
their pending build/UI checks had passed. Their descriptions retain that limit.

The earlier #38 report's 1,649 supplemental CPU passes and 21 software-Vulkan
passes belong to its frozen source `72612874ddb3eb47b63a486c070907efe0e3cf82`.
They are **historical**, not validation of #39, #40 or the new camera correction.
The required gate was already not green because of two x87 high-FIP before-state
prerequisites. Assertions were not weakened. Run the prerequisite checks in the
[roadmap](parity-roadmap-main.md) before describing this main revision as tested.

No original archive, executable, extracted asset, private URL, credential or
analysis database was added. Existing permitted screenshots were preserved;
none is presented as a new capture of this main revision.

## Follow-up decisions

- [Parity roadmap](parity-roadmap-main.md): available upstream bodies versus
  remaining reconstruction, ordered work packages and observable acceptance.
- [Rust compatibility boundaries](../architecture/rust-compatibility-boundaries.md):
  distinguish safe ownership, inert original addresses and actual assembly;
  migrate semantic APIs without losing callback or numeric behavior.
- [AGENTS.md](../../AGENTS.md): enforce those boundaries for new work and make
  review/runtime scope explicit.

No completed mission, original-runtime equivalence or coverage credit follows
from this integration alone.
