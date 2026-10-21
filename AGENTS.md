# Working conventions

- Use English code/documentation and Czech user communication. Preserve existing
  history/work; never force-push. Use the current isolated checkout rather than
  creating worktrees unless the user requests them.
- Keep gameplay in `game/` and `scenes/`, portable intake/validation in
  `importers/`, synthetic tests in `tests/`, and command wrappers in `scripts/`.
- Pin Godot and matching export templates to 4.6.3. Use Mobile/Vulkan on Linux;
  native Metal and signing/device verification require an Apple host.
- Keep original/converted assets, source URLs, credentials, BIOS, captures,
  analysis databases, and decompiled code outside Git and public exports. Private
  inputs live outside the checkout. Never print secret values or raw URL errors.
- Run `scripts/test.sh` and a Linux export after gameplay changes. Run the
  exported executable independently. Report logic tests, rendered checks, real
  input checks, and original-game comparisons separately; do not infer fidelity.
- Public fixtures must be synthetic. Do not copy unlicensed or decompiled code.
  Record copied licensed sources, exact revisions, licenses, and changes in
  `THIRD_PARTY_NOTICES.md`.
- The user requires an orchestrator that delegates implementation plus two
  sequential independent review rounds for every code milestone. Authors never
  review their own code. Delegate focused work to `gpt-6-luna` and complex work,
  integration and reviews to `gpt-6.1-sol`. Resolve findings and rerun relevant
  checks before Git
  milestones. Audit staged files and outgoing commits for private material.
- Record task-owned process identities in ignored/external ownership records.
  Stop children gracefully, wait for writes, then bounded TERM/KILL only for
  verified owned processes. Recover stale records by checking process identity
  before acting; never kill an application globally. Preserve inputs, saves,
  caches, tools, and useful private evidence. Remove only owned scratch files.

Onboarding must inspect current resources, network policy, available credentials,
and existing instructions before changing setup. Save reusable installation and
startup instructions through the environment configuration workflow. Repository
implementation changes require an authorized coding task; this project was
created under the user's explicit implementation request.
