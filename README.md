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

1. Create a branch.
2. Read the relevant package README and manifest.
3. Make a scoped change.
4. Run package-level validation.
5. Run repository-level checks when available.
6. Open a PR with validation results and production-boundary notes.
7. Deploy only an approved commit SHA.

The server is a deployment target, not an editing workspace. Do not edit docs, code,
scripts, wrappers, workers, runbooks, or runtime templates directly on the server or
inside `.hermes`.

## Documentation

Start from [docs/README.md](docs/README.md) for architecture, engineering rules, source
document inventory, migration policy, and operations references.

For product and Agent OS implementation context, read:

- [docs/product/agent-os-prd.md](docs/product/agent-os-prd.md)
- [docs/agent-os/README.md](docs/agent-os/README.md)
- [docs/operations/runtime-baseline.md](docs/operations/runtime-baseline.md)

## Migration

Migration status, source inventories, adoption order, and progress updates live in
`docs/plans/active/monorepo-migration.md`.

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
