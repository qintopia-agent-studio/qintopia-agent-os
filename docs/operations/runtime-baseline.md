# Runtime Baseline

Updated: 2026-07-03

This document summarizes the production runtime baseline from local and server read-only
documents. It is not a deployment runbook. Use it to decide what needs inventory,
templates, package ownership, and validation before migration.

## Current Runtime Shape

```text
QiWe / WeCom / Feishu / scheduled jobs
        |
        v
nginx, webhooks, Hermes gateways, sidecar services
        |
        v
Hermes profiles and profile-local plugins
        |
        v
Agent OS sidecar workers, Postgres, NATS, Feishu, knowledge stores
```

## Known Active Areas

| Area               | Current role                                         | Migration direction                                             |
| ------------------ | ---------------------------------------------------- | --------------------------------------------------------------- |
| Hermes gateways    | Profile-bound Agent runtime services                 | Convert config and units to templates                           |
| Erhua QiWe adapter | Current QiWe production path                         | First skill adoption candidate through `skills/qiwe`            |
| Message sidecar    | Message capture, data workers, context/MCP functions | Split across `runtime/sidecar`, `mcp/`, `workflows/`, `deploy/` |
| Postgres           | Data plane and fact store                            | Keep schemas and migrations under git                           |
| Feishu             | Human workbench and mirror                           | Keep adapters and field mapping governed                        |
| NATS               | Worker dependency                                    | Wrap as infrastructure dependency                               |
| nginx/systemd      | Runtime entry and service management                 | Template under `runtime/` and `deploy/`                         |

## Known Deprecated Or Legacy Areas

| Area                                        | Direction                                                                                            |
| ------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| WorkTool gateway and WorkTool Hermes plugin | Registered under `deprecated/`; remove only after owner-approved cleanup                             |
| OpenClaw QiWe adapter                       | Registered under `deprecated/openclaw`; keep only as rollback/audit reference until owner retires it |
| Hermes Kanban                               | Do not build new workflows on it                                                                     |
| Server-local one-off copies                 | Inventory first; do not copy wholesale into git                                                      |

## Server Handling Rules

- Read-only inventory is allowed.
- Direct server edits are not allowed.
- Runtime directories under `.hermes/profiles/*` are live state and cannot be copied
  wholesale.
- Server-local scripts, plugins, and backups need source path, hash, owner, and
  disposition before migration.
- Deployment must happen from reviewed commit SHAs through runbooks.

## Next Runtime Documentation Work

M3 establishes documentation entrypoints. Later migration phases should add:

- per-source inventory records
- systemd and nginx templates
- deploy dry-run output
- smoke check runbooks
- rollback notes
- secret and runtime-state exclusion rules per package
