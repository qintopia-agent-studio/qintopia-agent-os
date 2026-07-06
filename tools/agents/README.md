# Agent Checks

`tools/agents` owns helper scripts and checks for programming-agent collaboration.

`check-agents.mjs` validates the active Agent package contract.

Run:

```bash
pnpm agents:check
```

The check verifies:

- required active Agents are registered
- `xiaoqin` is not registered as an active Agent
- every registered Agent has `README.md`, `agent.yaml`, `profile.template.yaml`,
  `capabilities.md`, `runtime-notes.md`, and `docs/source-snapshot.md`
- profile templates include purpose, prompt sections, capabilities, forbidden actions,
  runtime mounts, excluded runtime state, and dry-run expectations
- package-like allowed capabilities point to active registry entries
- active Agents do not depend on deprecated packages
- `huabaosi` remains draft/review-pool until owner approval

## Pull Request Creation

Programming agents must not hand humans a prefilled GitHub compare URL as the normal PR
flow. Use the repository-owned `gh` workflow instead:

```bash
pnpm pr:doctor
pnpm pr:tools:check
pnpm pr:create -- --body-file /path/to/completed-pr-body.md
```

If GitHub CLI is missing, run:

```bash
pnpm pr:bootstrap
```

`pnpm pr:bootstrap -- --install` may install GitHub CLI on supported macOS, Windows, or
Debian/Ubuntu environments. Authentication still requires `gh auth login`.

PR bodies must start from `.github/PULL_REQUEST_TEMPLATE.md` and must fill Summary,
Planning, Domain, Validation, Production Boundary, Architecture / Tooling Boundary, and
Changelog. CI runs `pnpm pr:check-body` on pull requests and rejects empty template
sections.
