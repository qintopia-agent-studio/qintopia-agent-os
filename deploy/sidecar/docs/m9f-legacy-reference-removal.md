# M9-F Legacy Reference Removal

M9-F removes the remaining live references to the old standalone sidecar checkout. It is
a repointing step, not a feature expansion and not a cleanup window.

## Current Verified State

Read-only verification on 2026-07-04 confirmed:

| Area                                               | Current state                                           | M9-F target                                 |
| -------------------------------------------------- | ------------------------------------------------------- | ------------------------------------------- |
| `qintopia-agentos-member-profile-worker.service`   | active/enabled from `/home/ubuntu/qintopia-msg-sidecar` | monorepo checkout plus verified artifact    |
| `qintopia-agentos-graph-projection-worker.service` | active/enabled from `/home/ubuntu/qintopia-msg-sidecar` | monorepo checkout plus verified artifact    |
| `qintopia-agentos-raw-archive-worker.service`      | active/enabled from `/home/ubuntu/qintopia-msg-sidecar` | monorepo checkout plus verified artifact    |
| `qintopia-agentos-event-signal-worker.service`     | active/enabled from `/home/ubuntu/qintopia-msg-sidecar` | monorepo checkout plus verified artifact    |
| `qintopia-agentos-daily-digest-worker.service`     | active/enabled from `/home/ubuntu/qintopia-msg-sidecar` | monorepo checkout plus verified artifact    |
| `qintopia-agentos-daily-digest-publisher.service`  | active/enabled from `/home/ubuntu/qintopia-msg-sidecar` | monorepo checkout plus verified artifact    |
| Hermes `mcp-context` command                       | old wrapper path is still the known live command source | monorepo wrapper or release-managed wrapper |

## Scope

M9-F may update only the already-active legacy worker units listed above and the Hermes
`mcp-context` command path.

M9-F must not:

- Do not enable operations timers
- Do not enable real external send
- enable real workbench adapter integration
- remove or archive `/home/ubuntu/qintopia-msg-sidecar`
- delete WorkTool, Xiaoqin, OpenClaw, nginx, or migration directories
- edit Hermes profile runtime files without an owner-approved backup and diff

## Repository Checks

Run the M9-F readiness check before selecting a target SHA:

```bash
pnpm deploy:m9f:check
```

The check validates:

- the six legacy worker units render from the monorepo checkout and verified artifact
- rendered worker units do not reference `/home/ubuntu/qintopia-msg-sidecar`
- the Hermes MCP wrapper can run from artifact, release/current, or explicit
  `QINTOPIA_SIDECAR_BIN`
- M9-F docs mention rollback, archive deferral, and the external-send boundary

## Target Worker Shape

Render target units for review:

```bash
QINTOPIA_M9_TARGET_SHA="<approved-target-sha>" \
deploy/sidecar/scripts/render-systemd-units.sh
```

Each M9-F worker unit should use:

```text
WorkingDirectory=/home/ubuntu/qintopia-agent-os-monorepo
ExecStart=/home/ubuntu/qintopia-agent-os-artifacts/<approved-target-sha>/qintopia-message-sidecar <subcommand>
EnvironmentFile=/etc/qintopia/message-sidecar.env
Environment=QINTOPIA_SIDECAR_MIGRATIONS_DIR=/home/ubuntu/qintopia-agent-os-monorepo/runtime/postgres/migrations
Environment=QINTOPIA_DEPLOYED_COMMIT_SHA=<approved-target-sha>
```

Worker command mapping:

| Unit                                               | Command                             |
| -------------------------------------------------- | ----------------------------------- |
| `qintopia-agentos-member-profile-worker.service`   | `run-member-profile-worker`         |
| `qintopia-agentos-graph-projection-worker.service` | `run-graph-projection-worker`       |
| `qintopia-agentos-raw-archive-worker.service`      | `run-raw-archive-worker`            |
| `qintopia-agentos-event-signal-worker.service`     | `run-event-signal-worker`           |
| `qintopia-agentos-daily-digest-worker.service`     | `agentos-daily-digest-worker`       |
| `qintopia-agentos-daily-digest-publisher.service`  | `run-daily-digest-publisher-worker` |

Restart only services that were already active before the window.

## Hermes MCP Context

During M9-F, Hermes `mcp-context` should move away from:

```text
/home/ubuntu/qintopia-msg-sidecar/scripts/hermes/qintopia-context-mcp
```

The M9 transition command path is:

```text
/home/ubuntu/qintopia-agent-os-monorepo/deploy/sidecar/scripts/hermes/qintopia-context-mcp
```

The wrapper resolves the sidecar binary in this order:

1. `QINTOPIA_SIDECAR_BIN`
2. `/home/ubuntu/qintopia-agent-os-releases/current/sidecar/qintopia-message-sidecar`
3. `/home/ubuntu/qintopia-agent-os-artifacts/$QINTOPIA_DEPLOYED_COMMIT_SHA/qintopia-message-sidecar`
4. `/home/ubuntu/qintopia-agent-os-artifacts/current/qintopia-message-sidecar`

For the current M9 transition, provide either `QINTOPIA_DEPLOYED_COMMIT_SHA` or
`QINTOPIA_SIDECAR_BIN` in the approved Hermes MCP environment. For M10, prefer the
release/current path.

Do not point Hermes MCP config back to the old standalone checkout.

## Read-Only Preflight

Before a server mutation window:

```bash
for unit in \
  qintopia-agentos-member-profile-worker.service \
  qintopia-agentos-graph-projection-worker.service \
  qintopia-agentos-raw-archive-worker.service \
  qintopia-agentos-event-signal-worker.service \
  qintopia-agentos-daily-digest-worker.service \
  qintopia-agentos-daily-digest-publisher.service
do
  systemctl is-enabled "$unit" || true
  systemctl is-active "$unit" || true
  systemctl cat "$unit" | grep -E '^(WorkingDirectory|ExecStart|Environment=QINTOPIA_SIDECAR_MIGRATIONS_DIR)' || true
done

ps -eo pid=,args= | grep -F qintopia-context-mcp | grep -v grep || true
```

Record the output in the follow-up migration evidence.

## Apply Sequence

1. Confirm the target SHA has a successful `check` and `sidecar-artifact` workflow run.
2. Confirm the artifact is downloaded and verified on the server.
3. Back up the six current worker unit files and the Hermes MCP config file.
4. Render M9-F units and compare against current units.
5. Copy only the six approved worker units into `/etc/systemd/system`.
6. Run `sudo systemctl daemon-reload`.
7. Restart the six workers one by one, checking status and logs after each restart.
8. Repoint Hermes `mcp-context` command to the monorepo wrapper with an approved
   `QINTOPIA_DEPLOYED_COMMIT_SHA` or `QINTOPIA_SIDECAR_BIN`.
9. Restart only the affected Hermes profile process if the MCP command change requires
   it.
10. Re-run read-only reference checks and confirm no active process or active unit still
    references the old checkout.

## Rollback

Rollback restores the backed-up worker units and Hermes MCP config, then restarts only
the affected services or profile process.

Do not roll back by rebuilding on the server or by copying individual source files with
`scp`.

## Cleanup Deferral

M9-F does not archive or delete legacy directories. It only makes cleanup eligible for a
later owner-approved archive window. Archive or deletion work starts only after no
process, unit, timer, cron, MCP command, nginx route, or rollback path references the
old checkout.
