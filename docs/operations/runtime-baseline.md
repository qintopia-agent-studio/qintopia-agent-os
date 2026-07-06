# Runtime Baseline

Updated: 2026-07-04

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
| Agent profiles     | Active runtime identities and Agent behavior         | Registered under `agents/*`; copy only reviewed templates       |
| Erhua QiWe adapter | Current QiWe production path                         | First skill adoption candidate through `skills/qiwe`            |
| Message sidecar    | Message capture, data workers, context/MCP functions | Split across `runtime/sidecar`, `mcp/`, `workflows/`, `deploy/` |
| Postgres           | Data plane and fact store                            | Keep schemas and migrations under git                           |
| Feishu             | Human workbench and mirror                           | Keep adapters and field mapping governed                        |
| NATS               | Worker dependency                                    | Wrap as infrastructure dependency                               |
| nginx/systemd      | Runtime entry and service management                 | Template under `runtime/` and `deploy/`                         |

## Current Server Deployment State

The 2026-07-04 M9-D cutover moved only the first three sidecar services to the monorepo
checkout plus verified CI artifact. The server is still mixed:

| Runtime path                                     | Current role                                                                 |
| ------------------------------------------------ | ---------------------------------------------------------------------------- |
| `/home/ubuntu/qintopia-agent-os-monorepo`        | new deploy checkout for runbooks, scripts, migrations, and docs              |
| `/home/ubuntu/qintopia-agent-os-artifacts/<sha>` | transition path for verified CI sidecar binaries                             |
| `/home/ubuntu/qintopia-msg-sidecar`              | legacy checkout still used by six AgentOS workers and Hermes MCP context     |
| `/home/ubuntu/.hermes`                           | live Hermes runtime; not a release directory and not safe to copy wholesale  |
| `/home/ubuntu/.hermes/profiles/*`                | live profile state; versioned files must be extracted into templates/bundles |

The target deployment model is versioned releases:

```text
/home/ubuntu/qintopia-agent-os-releases/<approved-sha>
/home/ubuntu/qintopia-agent-os-releases/current
/home/ubuntu/qintopia-agent-os-releases/previous
```

Systemd services and Hermes-managed profile links should eventually point at `current`.
Rollback should switch `current` back to `previous` and restart the approved services.

## Known Deprecated Or Legacy Areas

| Area                                        | Direction                                                                                |
| ------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `/home/ubuntu/qintopia-msg-sidecar`         | remove only after M9-F migrates legacy workers and Hermes MCP context                    |
| `/home/ubuntu/qintopia-agent-os`            | review-pool only; do not use as source of truth                                          |
| `/home/ubuntu/qintopia-hermes-runtime`      | review for unique evidence, then archive or delete                                       |
| `/home/ubuntu/qintopia-migration`           | archive evidence or delete after owner review                                            |
| `/home/ubuntu/qintopia-worklog-guard-*`     | archive or delete after confirming no timer/process reference                            |
| WorkTool gateway and WorkTool Hermes plugin | Retired in M12-C; archive kept under `qintopia-agent-os-backups` for rollback/audit only |
| OpenClaw QiWe adapter                       | Retired in M12-B; archive kept under `qintopia-agent-os-backups` for rollback/audit only |
| Hermes Kanban                               | Do not build new workflows on it                                                         |
| Server-local one-off copies                 | Inventory first; do not copy wholesale into git                                          |

## Server Handling Rules

- Read-only inventory is allowed.
- Direct server edits are not allowed.
- Runtime directories under `.hermes/profiles/*` are live state and cannot be copied
  wholesale.
- Server-local scripts, plugins, and backups need source path, hash, owner, and
  disposition before migration.
- Deployment must happen from reviewed commit SHAs through runbooks.

## Next Runtime Documentation Work

Next migration phases should add:

- M9-F legacy worker and Hermes MCP path migration evidence
- release/current directory runbook and renderer support
- Hermes profile bundle rules for Erhua and other agents
- skill bundle rules for `skills/qiwe` and profile-local plugins
- systemd and nginx templates
- deploy dry-run output
- smoke check runbooks
- rollback notes
- secret and runtime-state exclusion rules per package
