# Collaboration Model

This repository is designed for human engineers and programming agents such as Codex and
Claude Code. The collaboration model is intentionally git-first and CI-backed.

## Default Flow

1. Branch from `master`.
2. Read the root entrypoint, agent instructions, and relevant package documentation.
3. Localize the change to one domain or package when possible.
4. Update package docs or manifests when the package contract changes.
5. Run focused validation, then repository checks.
6. Update `CHANGELOG.md` for repository-visible changes.
7. Update the active migration plan when migration state changes.
8. Open a PR with validation results and production-boundary notes.
9. Deploy only reviewed commit SHAs through a documented runbook.

## Programming Agent Read Order

For Codex, Claude Code, or another programming agent:

1. `README.md` or `README.zh-CN.md`
2. `AGENTS.md` or `AGENTS.zh-CN.md`
3. `CLAUDE.md` or `CLAUDE.zh-CN.md` when using Claude Code
4. [Documentation hub](../README.md)
5. [architecture/agent-os-overview.md](../architecture/agent-os-overview.md)
6. The target package README or manifest
7. The active plan when doing migration work

Agents should report what they read, what they plan to touch, validation commands, and
production boundaries before broad changes.

## PR Requirements

Every PR should state whether it touches:

- external message sends
- database writes or migrations
- Hermes profile runtime
- systemd, nginx, or deploy scripts
- Feishu, QiWe, or other external integrations
- secrets or runtime configuration

Docs-only PRs should still run `pnpm check`.

## CI/CD Direction

The initial CI gate runs repository formatting and Markdown linting. As packages are
adopted, CI should grow in this order:

1. manifest and registry validation
2. package-level tests and fixture replay
3. secret scanning
4. build checks for runtime packages
5. deploy dry-run checks
6. smoke checks for production-adjacent packages

Release jobs should deploy a reviewed commit SHA. They should not copy local uncommitted
files or hot-edit server checkouts.
