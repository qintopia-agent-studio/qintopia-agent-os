# M9-F Legacy Reference Removal

M9-F removed the remaining live references to the old standalone sidecar checkout. It
was a repointing step, not a feature expansion and not a cleanup window.

This document is now historical execution evidence and release/current reference
material. Current repository validation uses `pnpm deploy:release-model:check`.

## Pre-M9-F Verified State

Read-only verification on 2026-07-04 confirmed:

| Area                                               | Current state                                           | M9-F target                                 |
| -------------------------------------------------- | ------------------------------------------------------- | ------------------------------------------- |
| `qintopia-agentos-member-profile-worker.service`   | active/enabled from `/home/ubuntu/qintopia-msg-sidecar` | verified artifact, then release/current     |
| `qintopia-agentos-graph-projection-worker.service` | active/enabled from `/home/ubuntu/qintopia-msg-sidecar` | verified artifact, then release/current     |
| `qintopia-agentos-raw-archive-worker.service`      | active/enabled from `/home/ubuntu/qintopia-msg-sidecar` | verified artifact, then release/current     |
| `qintopia-agentos-event-signal-worker.service`     | active/enabled from `/home/ubuntu/qintopia-msg-sidecar` | verified artifact, then release/current     |
| `qintopia-agentos-daily-digest-worker.service`     | active/enabled from `/home/ubuntu/qintopia-msg-sidecar` | verified artifact, then release/current     |
| `qintopia-agentos-daily-digest-publisher.service`  | active/enabled from `/home/ubuntu/qintopia-msg-sidecar` | verified artifact, then release/current     |
| Hermes `mcp-context` command                       | old wrapper path is still the known live command source | monorepo wrapper or release-managed wrapper |

## Historical Scope

M9-F was allowed to update only the already-active legacy worker units listed above and
the Hermes `mcp-context` command path.

M9-F was not allowed to:

- Do not enable operations timers
- Do not enable real external send
- enable real workbench adapter integration
- remove or archive `/home/ubuntu/qintopia-msg-sidecar`
- delete WorkTool, Xiaoqin, OpenClaw, nginx, or migration directories
- edit Hermes profile runtime files without an owner-approved backup and diff
- fetch or checkout repository code as part of the routine artifact repoint

## Repository Checks

Run the stable release/current model check before selecting a target SHA:

```bash
pnpm deploy:release-model:check
```

The check validates:

- the six legacy worker units render from the monorepo checkout and verified artifact
- rendered worker units do not reference `/home/ubuntu/qintopia-msg-sidecar`
- the Hermes MCP wrapper can run from artifact, release/current, or explicit
  `QINTOPIA_SIDECAR_BIN`
- M9-F docs mention rollback, archive deferral, and the external-send boundary

The temporary M9-F check has been folded into the stable release/current deploy checks.

## Current Worker Shape

The release payload must come from COS. The runtime artifact and deploy bundle are
inputs for assembling `/home/ubuntu/qintopia-agent-os-releases/<release-sha>`. Workers
should point at `release/current`, not at deploy-bundle cache directories.

Render target units for review:

```bash
QINTOPIA_M9_TARGET_SHA="<approved-target-sha>" \
deploy/sidecar/scripts/render-systemd-units.sh
```

Each release/current worker unit should use:

```text
WorkingDirectory=/home/ubuntu/qintopia-agent-os-releases/current
ExecStart=/home/ubuntu/qintopia-agent-os-releases/current/sidecar/qintopia-message-sidecar <subcommand>
EnvironmentFile=/etc/qintopia/message-sidecar.env
Environment=QINTOPIA_SIDECAR_MIGRATIONS_DIR=/home/ubuntu/qintopia-agent-os-releases/current/runtime/postgres/migrations
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

During M9-F, Hermes `mcp-context` moved away from:

```text
/home/ubuntu/qintopia-msg-sidecar/scripts/hermes/qintopia-context-mcp
```

The M9 transition command path should come from the assembled release directory, not
from a live server git checkout:

```text
/home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/hermes/qintopia-context-mcp
```

The wrapper resolves the sidecar binary in this order:

1. `QINTOPIA_SIDECAR_BIN`
2. `/home/ubuntu/qintopia-agent-os-releases/current/sidecar/qintopia-message-sidecar`
3. `/home/ubuntu/qintopia-agent-os-artifacts/$QINTOPIA_DEPLOYED_COMMIT_SHA/qintopia-message-sidecar`
4. `/home/ubuntu/qintopia-agent-os-artifacts/current/qintopia-message-sidecar`

Prefer the release/current path. Provide `QINTOPIA_DEPLOYED_COMMIT_SHA` as evidence of
the approved runtime SHA, but do not rely on it to resolve the old checkout.

Do not point Hermes MCP config back to the old standalone checkout.

## Deploy Runner And Wrapper Boundary

M9-F has two separate concerns:

1. Runtime release: worker binaries must come from the COS-verified artifact or the
   future `release/current` directory.
2. Deploy files: scripts, renderers, and Hermes command wrappers must come from a
   reviewed deploy bundle and be assembled into the immutable release directory.

Do not mix these concerns. Server-side git checkout may be used only for diagnostics or
emergency bootstrap. It must not become the normal way to update wrapper, renderer, or
runtime code.

Before the M9-F execution window, download and verify two COS artifacts, then assemble a
release directory from them:

| Input          | COS artifact type | Staging path                                               | Release destination                                              |
| -------------- | ----------------- | ---------------------------------------------------------- | ---------------------------------------------------------------- |
| Runtime binary | `sidecar`         | `/tmp/qintopia-agent-os-artifacts/<runtime-sha>`           | `/home/ubuntu/qintopia-agent-os-releases/<release-sha>/sidecar/` |
| Deploy files   | `deploy-bundle`   | `/tmp/qintopia-agent-os-deploy-bundle/<deploy-bundle-sha>` | `/home/ubuntu/qintopia-agent-os-releases/<release-sha>/`         |

The current blocker is concrete: the live server deploy checkout was verified at
`94244504440a4f8fdb2eec07fd37b54db97fe368`, whose `qintopia-context-mcp` wrapper still
defaults to `/home/ubuntu/qintopia-msg-sidecar`. Do not repoint Hermes to that checkout.
Use the verified release wrapper instead.

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

1. Confirm the runtime SHA has a successful `check` and `sidecar-artifact` workflow run.
2. Confirm the deploy-bundle SHA has a successful `check` and `deploy-bundle-artifact`
   workflow run.
3. Confirm the runtime artifact is downloaded and verified on the server.
4. Confirm the deploy bundle is downloaded and verified on the server.
5. Assemble `/home/ubuntu/qintopia-agent-os-releases/<release-sha>` from the verified
   runtime artifact and deploy bundle payload.
6. Validate the release directory without changing `current`.
7. Back up the six current worker unit files and the Hermes MCP config file.
8. Update `previous` to the old `current` target, then atomically switch `current` to
   `<release-sha>`.
9. Render M9-F units from the release renderer and compare against current units.
10. Copy only the six approved worker units into `/etc/systemd/system`.
11. Run `sudo systemctl daemon-reload`.
12. Restart the six workers one by one, checking status and logs after each restart.
13. Repoint Hermes `mcp-context` command to the release wrapper.
14. Restart only the affected Hermes profile process if the MCP command change requires
    it.
15. Re-run read-only reference checks and confirm no active process or active unit still
    references the old checkout.

## Execution Window Checklist

Enter the M9-F mutation window only when all of these are true:

- The owner has approved the M9-F mutation window.
- The runtime artifact SHA and deploy bundle SHA are recorded separately.
- The target artifact has passed CI and server-side COS download/checksum validation.
- The deploy bundle has passed CI and server-side COS download/checksum validation.
- The six current worker unit files have been backed up.
- The affected Hermes MCP config files have been backed up.
- The rendered worker unit diff contains only the approved worker repoints.
- The assembled release wrapper does not contain `/home/ubuntu/qintopia-msg-sidecar`.
- Rollback commands and previous unit/config paths are visible in the operator shell.

Exit the window only after:

- All six previously active workers are active again.
- Affected Hermes profile processes are healthy if MCP config changed.
- No active process or active unit references `/home/ubuntu/qintopia-msg-sidecar`.
- No operations timers, external-send paths, or real workbench adapters were enabled.
- Evidence is recorded back into the migration plan and changelog.

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
