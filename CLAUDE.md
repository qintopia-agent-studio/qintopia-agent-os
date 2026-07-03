# Claude Code Instructions

This repository is the Qintopia Agent OS capability/plugin monorepo. Follow `AGENTS.md`
as the primary operating contract. This file highlights the rules most important for
Claude Code sessions.

## Start Here

Before editing:

1. Read `README.md`.
2. Read `AGENTS.md`.
3. Identify the target domain: `agents`, `skills`, `workflows`, `mcp`, `runtime`,
   `deploy`, `docs`, `fixtures`, `tools`, or `deprecated`.
4. Read the target package README or manifest if it exists.
5. If the task is a migration, read `docs/plans/active/monorepo-migration.md`.

## Core Boundaries

- Do not edit production servers directly.
- Do not copy live secrets or server-only runtime files into git.
- Do not treat server-side experiments as approved architecture.
- Do not build new workflows on WorkTool or Hermes Kanban.
- Do not organize new code by language at the top level.
- Do not add broad abstractions without a package owner, manifest, and validation path.

## Repository Shape

Use this model:

```text
agents/<agent>/          profile package
skills/<capability>/     reusable capability package
workflows/<workflow>/    governed business workflow
mcp/<adapter>/           MCP server or adapter
runtime/<area>/          runtime templates and render checks
deploy/<area>/           release, smoke, rollback
docs/<area>/             architecture and operating docs
fixtures/<area>/         replay and acceptance data
deprecated/<topic>/      historical POC only
```

Programming language is an implementation detail inside a package.

## Migration Guidance

When moving existing code into this monorepo:

1. Classify it as `adopt`, `template`, `runtime-only`, `deprecated`, or `remove`.
2. Preserve the source path and commit/hash in the package README or manifest.
3. Add or preserve tests/fixtures.
4. Keep production wiring disabled until registry, CI, smoke, and owner review are in
   place.

WorkTool material should go to `deprecated/` only when it has audit value.

## Reporting Format

For non-trivial work, report:

- files read
- files changed
- package/domain affected
- validation commands and results
- production boundary touched or not touched
- remaining risks

Keep implementation changes small and reviewable.
