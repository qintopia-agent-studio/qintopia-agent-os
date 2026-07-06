# Qintopia Agent OS Monorepo

[中文](README.zh-CN.md)

This repository is the source of truth for Qintopia Agent OS. It uses a
capability/plugin monorepo structure: directories are organized by Agent OS domain and
business capability, not by programming language.

## Purpose

Qintopia Agent OS coordinates Hermes profiles, governed skills, workflows, MCP adapters,
runtime templates, deployment scripts, fixtures, and engineering docs in one git
repository.

The goal is to replace the current mixed model where some code lives in separate repos,
some files are copied to the server, and some runtime assets are edited directly under
`.hermes`.

## Repository Map

```text
qintopia-agent-os-monorepo/
├── AGENTS.md                 # Codex and programming-agent rules
├── CLAUDE.md                 # Claude Code collaboration rules
├── README.md                 # Human entrypoint
├── registry/                 # Agent, skill, workflow, deployment indexes
├── agents/                   # One Agent profile package per directory
├── skills/                   # One reusable capability package per directory
├── workflows/                # Cross-Agent / cross-skill business flows
├── mcp/                      # MCP servers and adapters
├── runtime/                  # Runtime templates and generated-config rules
├── deploy/                   # Release manifests, scripts, smoke, rollback
├── docs/                     # Architecture, operations, product, reports
├── fixtures/                 # Replay fixtures and acceptance data
├── tools/                    # Inventory, registry, CI helper tools
└── deprecated/               # Historical POC material and removed paths
```

## Domain Rules

- `agents/`: personality, prompt, memory policy, allowed skills, forbidden actions, and
  profile-level tests.
- `skills/`: reusable capabilities such as QiWe, weather, Feishu Base, Postgres context,
  knowledge retrieval, and Qintopia business tools.
- `workflows/`: governed business processes such as Xiaoman activity signal, visual
  asset request, Erhua consultation, and daily operations.
- `mcp/`: MCP servers and adapters. Runtime credentials stay outside git.
- `runtime/`: Hermes, systemd, nginx, Postgres, and sidecar templates. This directory
  stores templates and render checks, not live server state.
- `deploy/`: release by reviewed commit SHA, smoke checks, rollback notes, and
  deployment manifests.
- `deprecated/`: WorkTool, WorkTool Hermes plugin, Hermes Kanban, OpenClaw, and other
  historical POC material kept only for audit or migration reference.

## Collaboration Model

All changes go through git:

1. Create a branch from `master`; do not develop directly on `master`.
2. Read the relevant package README and manifest.
3. Document first for new features, behavior changes, migrations, or runtime changes.
4. Make a scoped change.
5. Run package-level validation.
6. Run repository-level checks when available.
7. Commit with a Conventional Commits message, for example `feat: add weather skill` or
   `fix: resolve qintopia-tools skill path`.
8. Run `pnpm pr:doctor`, then open a PR with
   `pnpm pr:create -- --body-file <completed-pr-body.md>`.
9. Deploy only an approved commit SHA.

The server is a deployment target, not an editing workspace. Do not edit docs, code,
scripts, wrappers, workers, runbooks, or runtime templates directly on the server or
inside `.hermes`.

New implementation code must use the repository's existing language/tooling families:
TypeScript or JavaScript, Python, Rust, shell, SQL, YAML, JSON, and Markdown. Do not
introduce Java, Gradle, Maven, Kotlin, Go, Swift, C#, PHP, Ruby, Elixir, or another new
stack without an owner-approved architecture decision.

## Programming Agent Prompt

When using Codex, Claude Code, or another programming agent on this repository, start
the agent with this prompt:

```text
You are working in the Qintopia Agent OS monorepo.

Before editing, read README.md, AGENTS.md, docs/README.md,
docs/plans/active/current-roadmap.md, docs/engineering/programming-agent-guardrails.md,
docs/engineering/change-routing-index.md, and the README or manifest for the target
package.

Rules:
- Create a branch from master before changing files.
- Do not work directly on master.
- Document first for new features, behavior changes, migrations, runtime changes, or
  production-adjacent work.
- Organize code by Agent OS capability, not by programming language.
- Use only the existing implementation families: TypeScript/JavaScript, Python, Rust,
  shell, SQL, YAML, JSON, and Markdown.
- Do not introduce Java, Gradle, Maven, Kotlin, Go, Swift, C#, PHP, Ruby, Elixir, or a
  new toolchain without owner-approved architecture documentation.
- Do not edit production servers directly.
- Commit messages must follow Conventional Commits with approved types only.
- Create PRs with pnpm pr:doctor and pnpm pr:create; do not give humans a prefilled
  GitHub compare URL as the normal flow.
- Do not copy secrets, live .env files, Hermes live state, private logs, sessions, cache,
  auth files, raw chat logs, or runtime databases into git.
- Treat PR-Agent comments as advisory only; CI, CODEOWNERS, branch protection, and owner
  review remain authoritative.

For every change, report:
1. files and packages touched;
2. document or manifest updated before implementation;
3. validation commands and results;
4. whether production boundaries are touched;
5. rollback or decommission notes when runtime behavior changes.
```

## Documentation

Start from [docs/README.md](docs/README.md) for architecture, engineering rules, source
document inventory, migration policy, and operations references.

For product and Agent OS implementation context, read:

- [docs/plans/active/current-roadmap.md](docs/plans/active/current-roadmap.md)
- [docs/engineering/programming-agent-guardrails.md](docs/engineering/programming-agent-guardrails.md)
- [docs/engineering/change-routing-index.md](docs/engineering/change-routing-index.md)
- [docs/product/agent-os-prd.md](docs/product/agent-os-prd.md)
- [docs/agent-os/README.md](docs/agent-os/README.md)
- [docs/operations/runtime-baseline.md](docs/operations/runtime-baseline.md)

## Migration Archive

The monorepo migration and legacy cleanup are complete. Historical migration status,
source inventories, adoption order, and progress updates live in
[docs/plans/completed/monorepo-migration.md](docs/plans/completed/monorepo-migration.md).
Use [docs/plans/active/current-roadmap.md](docs/plans/active/current-roadmap.md) for
current work.

## Package Contract

Every future package should include:

- `README.md`: what this package does, owner, scope, and commands.
- `manifest.yaml` / `agent.yaml` / `workflow.yaml`: machine-readable metadata.
- `tests/` or `fixtures/`: replay or validation evidence.
- Clear production boundary notes.
- Rollback or decommission notes when it touches runtime behavior.

## Current Validation

Use the repository checks before opening a PR:

```bash
pnpm check
```
