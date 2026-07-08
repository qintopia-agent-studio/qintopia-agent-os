# M9.3 Sidecar Systemd Cutover Plan

This document defines the sidecar systemd render contract used by the M9 cutover and by
future release/current repoints. It is reference material, not approval to mutate the
server.

The original M9-D shape pointed directly at `qintopia-agent-os-artifacts/<sha>`. The
current production shape is immutable `qintopia-agent-os-releases/<sha>` directories
plus stable `current` and `previous` symlinks. See
`docs/operations/server-directory-plan.md` before adding new deploy automation.

The routine release source is the verified COS artifact for an approved commit SHA. Do
not make server-side `git fetch` or `git checkout` part of normal runtime repoints.
Server git access is only for deploy runner bootstrap, deploy script/template upgrades,
or explicit diagnostics.

## Goal

Keep sidecar services away from the standalone checkout and server-local release build:

```text
/home/ubuntu/qintopia-msg-sidecar
/home/ubuntu/qintopia-msg-sidecar/target/release/qintopia-message-sidecar
```

and on reviewed release-managed paths plus a verified CI artifact:

```text
/home/ubuntu/qintopia-agent-os-artifacts/<approved-target-sha>/qintopia-message-sidecar
```

Current release/current units should use:

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

M9-F included the remaining already-active `qintopia-agentos-*` workers that still
referenced `/home/ubuntu/qintopia-msg-sidecar`. Future repoints should restart only
already approved services and must not enable operations timers or real external adapter
paths as a side effect.

## Unit Contract

The old M9-D transition units used the deploy checkout only as the working directory and
migration source. The executable came from the verified artifact pulled from COS:

```text
WorkingDirectory=/home/ubuntu/qintopia-agent-os-monorepo
EnvironmentFile=/etc/qintopia/message-sidecar.env
Environment=QINTOPIA_SIDECAR_MIGRATIONS_DIR=/home/ubuntu/qintopia-agent-os-monorepo/runtime/postgres/migrations
ExecStart=/home/ubuntu/qintopia-agent-os-artifacts/<approved-target-sha>/qintopia-message-sidecar <subcommand>
```

The current release/current contract uses:

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
2. Download and verify the COS artifact and deploy bundle.
3. Assemble or verify `/home/ubuntu/qintopia-agent-os-releases/<approved-target-sha>`.
4. Run manifest, checksum, and binary checks from the release directory.
5. Render target units and compare against current server units.
6. Copy only owner-approved unit files into `/etc/systemd/system`.
7. Run `sudo systemctl daemon-reload`.
8. Restart `qintopia-message-sidecar.service`.
9. Check service status and recent journal.
10. Restart only previously active approved workers, one by one.
11. Run post-cutover smokes.

If the deploy checkout scripts themselves need to change, handle that as a separate
deploy runner upgrade before this sequence. Do not hide checkout updates inside a
runtime artifact repoint.

## Rollback Sequence

Historical M9-D rollback restored the previous unit files that pointed to:

```text
WorkingDirectory=/home/ubuntu/qintopia-msg-sidecar
ExecStart=/home/ubuntu/qintopia-msg-sidecar/target/release/qintopia-message-sidecar <subcommand>
```

Current release/current rollback should switch `current` back to `previous`, restore any
changed unit files only if needed, then:

```bash
sudo systemctl daemon-reload
sudo systemctl restart qintopia-message-sidecar.service
systemctl status qintopia-message-sidecar.service --no-pager
journalctl -u qintopia-message-sidecar.service -n 100 --no-pager
```

Rollback is operational rollback. If any approved database-write smoke or migration runs
during the window, database rollback must follow its own reviewed note.
