# Programming Agent Guardrails

This document tells Codex, Claude Code, and similar programming agents how to work in
this repository without drifting away from the Agent OS architecture.

## Start Protocol

Before editing, an agent must read:

1. `README.md` or `README.zh-CN.md`
2. `AGENTS.md` or `AGENTS.zh-CN.md`
3. `docs/README.md` or `docs/README.zh-CN.md`
4. `docs/plans/active/current-roadmap.md`
5. `docs/engineering/change-routing-index.md`
6. the target package README or manifest

For runtime, server, deployment, Hermes profile, Feishu, QiWe, or database work, also
read the relevant document under `docs/operations/` or `docs/engineering/`.

## Branch Rule

Do not develop directly on `master`.

- Create a branch before editing.
- Keep each branch scoped to one package, domain, or documented plan.
- CI may run on `master` after merge, but local feature work should not happen there.

## Commit Message Rule

Use Conventional Commits for every commit. Allowed types are:

```text
build chore ci docs feat fix perf refactor revert style test
```

Use `feat` for new capabilities, `fix` for bug fixes, `docs` for documentation-only
changes, `ci` for CI/check gates, `test` for tests or fixtures, `refactor` for
behavior-preserving code movement, `build` for dependency or artifact tooling, and
`chore` for maintenance. Do not invent custom types. Commit messages are checked by the
local `commit-msg` hook and by CI.

## Documentation-First Rule

For new features, behavior changes, migrations, or runtime changes:

1. Write or update the relevant doc first.
2. State the domain, goal, scope, production boundary, and validation path.
3. Then implement code.
4. Update the doc again if implementation changes the design.

Small typo fixes and purely mechanical formatting do not need a design note.

## Language And Toolchain Rule

Allowed implementation families are the ones already used by this repository:

- TypeScript or JavaScript
- Python
- Rust
- shell scripts
- SQL
- YAML, JSON, and Markdown for configuration and docs

Do not introduce Java, Gradle, Maven, Kotlin, Go, Swift, C#, PHP, Ruby, Elixir, or
another new language/toolchain stack without an explicit owner-approved architecture
decision.

Implementation language is always package-internal. Do not create top-level language
directories.

## Package Rule

New behavior belongs in a package:

- Agent profile behavior: `agents/<agent>/`
- reusable capability: `skills/<capability>/`
- cross-Agent process: `workflows/<workflow>/`
- MCP adapter/server: `mcp/<adapter>/`
- runtime templates/checks: `runtime/<area>/`
- release/smoke/rollback: `deploy/<area>/`
- historical or retired material: `deprecated/<topic>/`

Every package should have a README, manifest, validation command, owner/risk metadata,
and production-boundary note.

## Production Boundary Rule

Do not change production behavior casually. Explicitly call out whether the PR touches:

- external message sends
- database writes or migrations
- Hermes profile runtime
- systemd, nginx, deploy scripts, or release promotion
- Feishu, QiWe, Tencent COS, GitHub Actions, or other external integrations
- secrets or runtime config

Server edits must go through reviewed artifacts and runbooks. Do not hot-edit server
code, docs, `.hermes` files, or systemd units.

## PR Review Automation Rule

PR-Agent is an advisory reviewer only.

- Treat PR-Agent comments as review input, not approval.
- Do not use PR-Agent output to bypass CI, CODEOWNERS, branch protection, or owner
  review.
- If PR-Agent suggests an architecture direction that conflicts with repository docs,
  follow the repository docs and update the docs only through a normal PR.
- See `docs/engineering/pr-agent-review.md` for the workflow boundary.

## PR Creation Rule

Programming agents must create PRs through the repository-owned GitHub CLI flow, not by
handing a human a prefilled GitHub compare URL.

Use:

```bash
pnpm pr:doctor
pnpm pr:create -- --body-file <completed-pr-body.md>
```

The body file must start from `.github/PULL_REQUEST_TEMPLATE.md` and fill every required
section. CI runs `pnpm pr:check-body` on pull requests and rejects empty template
bodies.

If `gh` is missing, run `pnpm pr:bootstrap` to print supported installation commands. On
supported environments, `pnpm pr:bootstrap -- --install` may install GitHub CLI.
Authentication still requires `gh auth login` only when the actual PR flow reports an
authentication failure.

In the Codex desktop environment, do not run extra GitHub authentication checks before
creating a PR. Use `pnpm pr:create` directly after PR readiness checks; only handle
authentication when the actual push or PR creation command fails.

## Hermes Profile Rule

Hermes profile live state is not source code.

Keep these outside git:

- `.env`
- sessions, logs, cache, pairing, auth, locks
- generated memory and state databases
- private chat logs and raw member profile data

Treat reviewed profile distribution files such as `SOUL.md`, skills, cron, and MCP
declarations as future bundle inputs, not as live server state to copy wholesale.

## Stop Conditions

Stop and ask for owner confirmation before:

- introducing a new programming language or build system
- enabling real external sends
- broadening Feishu/QiWe permissions
- replacing or symlinking `SOUL.md` or `config.yaml` for a live Hermes profile
- deleting archives or rollback material permanently
- reviving WorkTool, OpenClaw, Hermes Kanban, or current WorkTool-bound Xiaoqin runtime
