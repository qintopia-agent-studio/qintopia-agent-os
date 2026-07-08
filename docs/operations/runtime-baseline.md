# Runtime Baseline

Updated: 2026-07-08

This document summarizes the production runtime baseline after the monorepo migration,
M9 sidecar release/current cutover, M10 package adoption, and M12 legacy archiving. It
is not a deployment runbook. Use it to decide what still needs package ownership,
profile/plugin bundle rules, validation, or owner-approved cleanup.

## Current Runtime Shape

```text
QiWe / WeCom / Feishu / scheduled jobs
        |
        v
nginx, webhooks, Hermes gateways, release/current sidecar services
        |
        v
Hermes profiles and profile-local plugins
        |
        v
Agent OS sidecar workers, Postgres, NATS, Feishu, knowledge stores
```

## Known Active Areas

| Area               | Current role                                         | Migration direction                                                              |
| ------------------ | ---------------------------------------------------- | -------------------------------------------------------------------------------- |
| Hermes gateways    | Profile-bound Agent runtime services                 | Convert config and units to templates                                            |
| Agent profiles     | Active runtime identities and Agent behavior         | Registered under `agents/*`; copy only reviewed templates                        |
| Erhua QiWe adapter | Current QiWe production path                         | First skill adoption candidate through `skills/qiwe`                             |
| Message sidecar    | Message capture, data workers, context/MCP functions | Release/current-managed under `runtime/sidecar`, `mcp/`, `workflows/`, `deploy/` |
| Postgres           | Data plane and fact store                            | Keep schemas and migrations under git                                            |
| Feishu             | Human workbench and mirror                           | Keep adapters and field mapping governed                                         |
| NATS               | Worker dependency                                    | Wrap as infrastructure dependency                                                |
| nginx/systemd      | Runtime entry and service management                 | Template under `runtime/` and `deploy/`                                          |

## Current Server Deployment State

M9-F completed the active sidecar runtime cutover. All nine sidecar service-family
processes and the Hermes `qintopia-context` MCP command now run through immutable
release directories and the stable `current` symlink. The server remains transitional
only for profile/plugin bundle rollout, deploy-runner/bootstrap diagnostics, and
archive-retention cleanup.

| Runtime path                                       | Current role                                                                    |
| -------------------------------------------------- | ------------------------------------------------------------------------------- |
| `/home/ubuntu/qintopia-agent-os-releases/current`  | active release pointer for sidecar services, workers, MCP wrappers, and bundles |
| `/home/ubuntu/qintopia-agent-os-releases/previous` | rollback pointer, populated by release promotion when a prior release exists    |
| `/home/ubuntu/qintopia-agent-os-artifacts/<sha>`   | verified artifact cache and audit evidence; not a service working directory     |
| `/home/ubuntu/qintopia-agent-os-monorepo`          | diagnostic/bootstrap checkout; not the normal runtime release source            |
| `/home/ubuntu/.hermes`                             | live Hermes runtime; not a release directory and not safe to copy wholesale     |
| `/home/ubuntu/.hermes/profiles/*`                  | live profile state; versioned files must be extracted into templates/bundles    |

The active deployment model is versioned releases:

```text
/home/ubuntu/qintopia-agent-os-releases/<approved-sha>
/home/ubuntu/qintopia-agent-os-releases/current
/home/ubuntu/qintopia-agent-os-releases/previous
```

Systemd services and Hermes-managed profile links should point at `current`. Rollback
switches `current` back to `previous` and restarts only the approved services or Hermes
profile processes.

## Known Deprecated Or Legacy Areas

| Area                                        | Direction                                                                                |
| ------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `/home/ubuntu/qintopia-msg-sidecar`         | archived in M12 low-risk batch; retain archive until owner-approved deletion             |
| `/home/ubuntu/qintopia-agent-os`            | archived in M12 low-risk batch; do not use as source of truth                            |
| `/home/ubuntu/qintopia-hermes-runtime`      | archived in M12 low-risk batch; retain archive until owner-approved deletion             |
| `/home/ubuntu/qintopia-migration`           | archived in M12 low-risk batch; retain archive until owner-approved deletion             |
| `/home/ubuntu/qintopia-worklog-guard-*`     | archived in M12 low-risk batch; retain archive until owner-approved deletion             |
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

Current follow-up documentation should focus on:

- Hermes profile bundle rules for Erhua and other agents.
- Skill bundle rules for `skills/qiwe` and profile-local plugins.
- External adapter allowlist and rollback evidence before real sends or workbench
  integrations are enabled.
- Archive retention policy for M12 backups before any permanent deletion.
- Secret and runtime-state exclusion rules per package.
