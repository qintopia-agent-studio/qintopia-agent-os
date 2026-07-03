# Sidecar Monorepo Cutover Plan

This plan describes how to move production sidecar deployment from the standalone
`qintopia-msg-sidecar` checkout to this monorepo. It is a plan, not an approved deploy
runbook.

For the full M9 migration window, rollback, deprecated runtime cleanup, and acceptance
contract, use `../../../docs/operations/m9-server-cutover-runbook.md`.

## Current Production Model

- Server checkout: `/home/ubuntu/qintopia-msg-sidecar`
- Existing deploy script snapshot: `deploy/sidecar/scripts/server-deploy.sh`
- Existing service binary:
  `/home/ubuntu/qintopia-msg-sidecar/target/release/qintopia-message-sidecar`
- Existing runtime env file: `/etc/qintopia/message-sidecar.env`
- Known caveat from 2026-07-03: the server checkout was on
  `codex/huabaosi-localization-shadow@b16c247a19ec751c08de75ae2d312f35b765f317`, not
  local `main@eda2652f21999e4f32699463413372accbd3b76e`.

The Huabaosi shadow branch is review-pool input, not an approved production roadmap
item.

## Target Production Model

- Server checkout: `/home/ubuntu/qintopia-agent-os-monorepo`
- Git branch: `master`
- Build root: repository root
- Sidecar crate: `runtime/sidecar`
- Migration source: `runtime/postgres/migrations`
- Runtime env file: `/etc/qintopia/message-sidecar.env`
- Binary after build:
  `/home/ubuntu/qintopia-agent-os-monorepo/runtime/sidecar/target/release/qintopia-message-sidecar`

Systemd units should point to the monorepo checkout only after the target commit has
passed local CI, server build, smoke, and rollback checks.

## Cutover Preconditions

- Owner approves the cutover window and target commit SHA.
- Server checkout is clean before any git operation.
- Huabaosi shadow branch has been explicitly classified as one of:
  - adopted into monorepo
  - kept in review-pool
  - discarded
- `pnpm check` passes locally on the exact target commit.
- `cargo build --release --locked --manifest-path runtime/sidecar/Cargo.toml` passes on
  the server.
- `deploy/sidecar/scripts/operations-control-plane-smoke.sh` passes on the server.
- Apply smokes that write Postgres are explicitly approved before they run.
- Rollback command and previous standalone commit are recorded before service changes.

## Proposed Cutover Sequence

1. Read-only server verification:

   ```bash
   cd /home/ubuntu/qintopia-msg-sidecar
   git status --short --branch
   git rev-parse HEAD
   systemctl status qintopia-message-sidecar.service --no-pager
   systemctl status qintopia-message-embedding-worker.service --no-pager
   ```

2. Prepare monorepo checkout:

   ```bash
   cd /home/ubuntu
   git clone <approved-monorepo-remote> qintopia-agent-os-monorepo
   cd /home/ubuntu/qintopia-agent-os-monorepo
   git checkout master
   git rev-parse HEAD
   pnpm install --frozen-lockfile
   pnpm check
   ```

3. Build sidecar from monorepo:

   ```bash
   cargo build --release --locked --manifest-path runtime/sidecar/Cargo.toml
   ```

4. Validate without service cutover:

   ```bash
   set -a
   . /etc/qintopia/message-sidecar.env
   set +a
   runtime/sidecar/target/release/qintopia-message-sidecar check
   deploy/sidecar/scripts/operations-control-plane-smoke.sh
   deploy/sidecar/scripts/xiaoman-activity-acceptance-smoke.sh
   ```

5. Install or update systemd units to point to the monorepo binary and working
   directory.

   The service should use:

   ```text
   WorkingDirectory=/home/ubuntu/qintopia-agent-os-monorepo
   ExecStart=/home/ubuntu/qintopia-agent-os-monorepo/runtime/sidecar/target/release/qintopia-message-sidecar run
   ```

   Worker units should use the same binary and explicit subcommands.

6. Restart and verify:

   ```bash
   sudo systemctl daemon-reload
   sudo systemctl restart qintopia-message-sidecar.service
   systemctl status qintopia-message-sidecar.service --no-pager
   journalctl -u qintopia-message-sidecar.service -n 100 --no-pager
   ```

7. Run post-cutover smoke:

   ```bash
   set -a
   . /etc/qintopia/message-sidecar.env
   set +a
   runtime/sidecar/target/release/qintopia-message-sidecar check
   deploy/sidecar/scripts/operations-control-plane-smoke.sh
   ```

## Rollback

Rollback must return systemd units to the old standalone checkout and restart the
affected services.

```bash
sudo systemctl stop qintopia-message-sidecar.service
sudo systemctl edit --full qintopia-message-sidecar.service
# restore WorkingDirectory=/home/ubuntu/qintopia-msg-sidecar
# restore ExecStart=/home/ubuntu/qintopia-msg-sidecar/target/release/qintopia-message-sidecar run
sudo systemctl daemon-reload
sudo systemctl start qintopia-message-sidecar.service
systemctl status qintopia-message-sidecar.service --no-pager
```

If migrations ran during the cutover, rollback is operational rollback only. Database
state must be handled by a separate migration rollback note before any destructive
change is allowed.

## What Not To Do

- Do not edit files directly under either server checkout.
- Do not use `scp` to overwrite individual source files.
- Do not deploy from a dirty worktree.
- Do not treat the Huabaosi shadow branch as approved by copying it into production.
- Do not run guarded apply smokes unless the owner explicitly approves Postgres writes.
- Do not change Hermes profiles as part of sidecar cutover unless that profile change
  has its own reviewed plan.
