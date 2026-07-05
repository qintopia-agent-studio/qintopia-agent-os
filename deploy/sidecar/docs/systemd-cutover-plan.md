# M9.3 Sidecar Systemd Cutover Plan

This document defines the systemd target shape for the M9 sidecar cutover. It is
preparation material, not approval to mutate the server.

The M9 shape below is a transition model: units point directly at
`qintopia-agent-os-artifacts/<sha>`. The target direction for M10 is immutable
`qintopia-agent-os-releases/<sha>` directories plus stable `current` and `previous`
symlinks. See `docs/operations/server-directory-plan.md` before adding new deploy
automation.

The routine release source is the verified COS artifact for an approved commit SHA. Do
not make server-side `git fetch` or `git checkout` part of normal runtime repoints.
Server git access is only for deploy runner bootstrap, deploy script/template upgrades,
or explicit diagnostics.

## Goal

Move sidecar services away from the standalone checkout and server-local release build:

```text
/home/ubuntu/qintopia-msg-sidecar
/home/ubuntu/qintopia-msg-sidecar/target/release/qintopia-message-sidecar
```

to reviewed release-managed paths plus a verified CI artifact:

```text
/home/ubuntu/qintopia-agent-os-artifacts/<approved-target-sha>/qintopia-message-sidecar
```

After M9-F removes all live references to `/home/ubuntu/qintopia-msg-sidecar`, the next
deployment iteration should move from artifact paths to release paths:

```text
/home/ubuntu/qintopia-agent-os-releases/current
/home/ubuntu/qintopia-agent-os-releases/current/sidecar/qintopia-message-sidecar
```

## Non-Mutating Preview

Render the target unit files locally:

```bash
pnpm deploy:systemd:check
```

Render review files into `dist/` for manual inspection:

```bash
QINTOPIA_M9_TARGET_SHA="<approved-target-sha>" \
deploy/sidecar/scripts/render-systemd-units.sh
```

The renderer refuses to write into `/etc/systemd/system`. During the approved migration
window, the operator must copy reviewed files intentionally and record the diff.

## Target Unit Set

The renderer produces the full known sidecar service family:

| Unit                                                         | Default M9 action                           |
| ------------------------------------------------------------ | ------------------------------------------- |
| `qintopia-message-sidecar.service`                           | update and restart first                    |
| `qintopia-message-embedding-worker.service`                  | update only if already active               |
| `qintopia-message-identity-worker.service`                   | update only if already active               |
| `qintopia-agentos-member-profile-worker.service`             | update only if already active               |
| `qintopia-agentos-graph-projection-worker.service`           | update only if already active               |
| `qintopia-agentos-event-signal-worker.service`               | update only if already active               |
| `qintopia-agentos-daily-digest-worker.service`               | update only if already active               |
| `qintopia-agentos-daily-digest-publisher.service`            | update only if already active               |
| `qintopia-agentos-raw-archive-worker.service`                | update only if already active               |
| `qintopia-agentos-operations-workflow-sync.service/timer`    | render for review; do not enable by default |
| `qintopia-agentos-operations-workbench-event.service/timer`  | render for review; do not enable by default |
| `qintopia-agentos-operations-group-send-ready.service/timer` | render for review; do not enable by default |

M9 is a cutover, not a worker expansion. Do not enable a service or timer that was not
already active unless the owner explicitly approves that addition during the window.

M9-F should include the remaining already-active `qintopia-agentos-*` workers that still
reference `/home/ubuntu/qintopia-msg-sidecar`. It should not enable operations timers or
real external adapter paths.

## Unit Contract

M9 transition units use the deploy checkout only as the working directory and migration
source. The executable comes from the verified artifact pulled from COS:

```text
WorkingDirectory=/home/ubuntu/qintopia-agent-os-monorepo
EnvironmentFile=/etc/qintopia/message-sidecar.env
Environment=QINTOPIA_SIDECAR_MIGRATIONS_DIR=/home/ubuntu/qintopia-agent-os-monorepo/runtime/postgres/migrations
ExecStart=/home/ubuntu/qintopia-agent-os-artifacts/<approved-target-sha>/qintopia-message-sidecar <subcommand>
```

The M10 release/current contract should use:

```text
WorkingDirectory=/home/ubuntu/qintopia-agent-os-releases/current
EnvironmentFile=/etc/qintopia/message-sidecar.env
Environment=QINTOPIA_SIDECAR_MIGRATIONS_DIR=/home/ubuntu/qintopia-agent-os-releases/current/runtime/postgres/migrations
Environment=QINTOPIA_DEPLOYED_COMMIT_SHA=<approved-target-sha>
ExecStart=/home/ubuntu/qintopia-agent-os-releases/current/sidecar/qintopia-message-sidecar <subcommand>
```

The identity worker may also use:

```text
EnvironmentFile=-/home/ubuntu/.hermes/profiles/erhua/.env
```

Target units must not:

- run `cargo`
- fetch git remotes
- build on the server
- reference `/home/ubuntu/qintopia-msg-sidecar`
- reference `target/release/qintopia-message-sidecar`

## Apply Sequence During Approved Window

1. Confirm the target SHA has a successful CI run with `check` and `sidecar-artifact`.
2. Download and verify the COS artifact into
   `/home/ubuntu/qintopia-agent-os-artifacts/<approved-target-sha>`.
3. Run manifest, checksum, and binary checks from the downloaded artifact.
4. Render target units and compare against current server units.
5. Copy only owner-approved unit files into `/etc/systemd/system`.
6. Run `sudo systemctl daemon-reload`.
7. Restart `qintopia-message-sidecar.service`.
8. Check service status and recent journal.
9. Restart only previously active approved workers, one by one.
10. Run post-cutover smokes.

If the deploy checkout scripts themselves need to change, handle that as a separate
deploy runner upgrade before this sequence. Do not hide checkout updates inside a
runtime artifact repoint.

## Rollback Sequence

Rollback restores the previous unit files that point to:

```text
WorkingDirectory=/home/ubuntu/qintopia-msg-sidecar
ExecStart=/home/ubuntu/qintopia-msg-sidecar/target/release/qintopia-message-sidecar <subcommand>
```

Then:

```bash
sudo systemctl daemon-reload
sudo systemctl restart qintopia-message-sidecar.service
systemctl status qintopia-message-sidecar.service --no-pager
journalctl -u qintopia-message-sidecar.service -n 100 --no-pager
```

Rollback is operational rollback. If any approved database-write smoke or migration runs
during the window, database rollback must follow its own reviewed note.
