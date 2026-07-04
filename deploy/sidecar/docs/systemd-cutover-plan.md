# M9.3 Sidecar Systemd Cutover Plan

This document defines the monorepo-native systemd target shape for the M9 sidecar
cutover. It is preparation material, not approval to mutate the server.

## Goal

Move sidecar services away from the standalone checkout and server-local release build:

```text
/home/ubuntu/qintopia-msg-sidecar
/home/ubuntu/qintopia-msg-sidecar/target/release/qintopia-message-sidecar
```

to a reviewed monorepo checkout plus a verified CI artifact:

```text
/home/ubuntu/qintopia-agent-os-monorepo
/home/ubuntu/qintopia-agent-os-artifacts/<approved-target-sha>/qintopia-message-sidecar
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

## Unit Contract

Every target service must use:

```text
WorkingDirectory=/home/ubuntu/qintopia-agent-os-monorepo
EnvironmentFile=/etc/qintopia/message-sidecar.env
Environment=QINTOPIA_SIDECAR_MIGRATIONS_DIR=/home/ubuntu/qintopia-agent-os-monorepo/runtime/postgres/migrations
ExecStart=/home/ubuntu/qintopia-agent-os-artifacts/<approved-target-sha>/qintopia-message-sidecar <subcommand>
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
2. Clone or update `/home/ubuntu/qintopia-agent-os-monorepo` to the approved SHA.
3. Download and verify the CI artifact into
   `/home/ubuntu/qintopia-agent-os-artifacts/<approved-target-sha>`.
4. Render target units and compare against current server units.
5. Copy only owner-approved unit files into `/etc/systemd/system`.
6. Run `sudo systemctl daemon-reload`.
7. Restart `qintopia-message-sidecar.service`.
8. Check service status and recent journal.
9. Restart only previously active approved workers, one by one.
10. Run post-cutover smokes.

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
