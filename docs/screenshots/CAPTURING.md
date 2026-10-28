# Capture a public Lab scene

From a clean, committed checkout with the pinned toolchain and a working
Vulkan/Metal adapter:

```sh
python3 scripts/capture_lab.py --output /tmp/lhr-lab-capture
```

This runs the actual `scripts/run.sh` for `gallery`, `courtyard`, `primitives`
and `lights`. Select a subset with repeated `--scene` arguments. The default
300-second timeout applies to each build/capture and is configurable.

The output directory must be new and outside the repository. It contains PNGs,
local build/runtime logs and `receipt.json` with the source commit, platform,
dimensions and image SHA-256 hashes. Dirty, sparse or assume-unchanged sources,
tracked symlinks/submodules, source changes during capture, unsuccessful processes
and incomplete PNG framing are rejected. A failed batch keeps local evidence
but produces no success receipt.
Timeout cleanup sends TERM, allows a three-second grace, then sends KILL before
reaping its exclusively owned session leader. It never signals an already reaped
PID; the final wait is bounded. The runner requires default SIGCHLD handling.

The receipt verifies ordinary tracked files at capture boundaries, not a hermetic
build. Host libraries, ignored files, Cargo configuration and the environment are
not attested by the source commit; native build/runtime validation is separate.

These are **headless synthetic scene-only captures at time zero**, not screenshots
of the UI panel or authored gameplay. The receipt checks PNG framing and chunk
checksums, not decoded pixels or visual correctness. Inspect every image before
publishing it. Put the exact source revision and capture mode in the PR caption;
do not reuse an old image as evidence for a new revision.

Copy only reviewed PNGs and their receipt into a visual PR. Keep raw logs local:
they can contain machine paths. This helper reads no original installation and
never fabricates a replacement for a failed render. Original-game captures still
need a separately documented local run and must not include extracted assets.

Runner unit tests use tiny authored PNGs and mocked renders; passing those tests
is not evidence that any GPU scene was rendered.
