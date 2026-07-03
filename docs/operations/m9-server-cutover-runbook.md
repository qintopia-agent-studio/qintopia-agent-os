# M9 Server Cutover Runbook

M9 moves production deployment from scattered server checkouts and ad hoc updates to
this monorepo. This document is the execution contract for the final migration window.
It is not permission to mutate the server before the owner approves a cutover.

## Scope

M9 covers:

- deploying an approved `master` commit SHA from this monorepo
- wiring the sidecar service family to the monorepo checkout
- running non-mutating smoke checks and explicitly approved database-write smokes
- archiving or removing deprecated WorkTool, Xiaoqin WorkTool, and OpenClaw runtime
  assets only after owner approval
- recording rollback evidence and post-cutover status in git

M9 does not cover:

- approving the Huabaosi shadow branch as product direction
- editing Hermes profile runtime files directly
- replacing server files through `scp`
- changing production secrets or `.env` values in git
- publishing or sending external messages outside approved smoke boundaries

## Required Inputs

Fill these before the migration window:

| Field                  | Value                                                        |
| ---------------------- | ------------------------------------------------------------ |
| Owner approver         | PatrickLiveCool                                              |
| Migration operator     | TBD                                                          |
| Reviewers              | detroxryo, noraincode, PatrickLiveCool, qiaopengjun5162      |
| Monorepo remote        | `git@github.com:qintopia-agent-studio/qintopia-agent-os.git` |
| Target branch          | master                                                       |
| Target commit SHA      | TBD                                                          |
| Previous sidecar SHA   | TBD                                                          |
| Server start time      | TBD                                                          |
| Rollback decision time | TBD                                                          |
| Rollback owner         | PatrickLiveCool                                              |

The target SHA must pass:

```bash
pnpm check
pnpm deploy:preflight
```

## Preflight Dry Run

2026-07-03 read-only preflight results:

| Check                 | Result                                                                             |
| --------------------- | ---------------------------------------------------------------------------------- |
| Local remote          | `origin` points to `git@github.com:qintopia-agent-studio/qintopia-agent-os.git`    |
| Local pushed branch   | `master` pushed successfully                                                       |
| Local HEAD at check   | `a0658e28aee570c56ce1a932ff68b84b43fbbacb`                                         |
| GitHub default branch | `master`                                                                           |
| Server target path    | `/home/ubuntu/qintopia-agent-os-monorepo` missing                                  |
| Server GitHub access  | blocked: current server SSH identity cannot read the private repo                  |
| Server Node.js        | missing from `PATH`                                                                |
| Server pnpm           | missing from `PATH`                                                                |
| Server Rust           | `cargo 1.96.1`, `rustc 1.96.1` observed                                            |
| Server disk           | `/` at 91% used, about 5.6G available                                              |
| Current sidecar       | active, enabled, running from `/home/ubuntu/qintopia-msg-sidecar`                  |
| Current sidecar SHA   | `b16c247a19ec751c08de75ae2d312f35b765f317` on `codex/huabaosi-localization-shadow` |

Blocking items before cutover:

1. Add a deploy path for the private GitHub repo. Prefer a dedicated read-only deploy
   key for `qintopia-agent-studio/qintopia-agent-os`; do not reuse legacy deploy keys if
   GitHub rejects them or they are tied to another repository.
2. Install or expose Node.js and pnpm on the server so `pnpm install --frozen-lockfile`,
   `pnpm check`, and `pnpm deploy:preflight` can run from the monorepo checkout.
3. Confirm available disk is sufficient for the monorepo checkout, dependencies, and
   Rust release build.
4. Reconfirm whether the Huabaosi shadow branch should remain review-pool before the
   active service is repointed.

## Pre-Cutover Freeze

Before any server mutation:

1. Confirm direct server edits for Agent OS, sidecar, Hermes profile templates, and
   legacy WorkTool/OpenClaw paths remain frozen.
2. Confirm no one is making `scp` source updates, hotfixes, or direct server commits.
3. Confirm local `master` is clean and pushed to the approved remote.
4. Confirm the server can fetch the approved monorepo remote.
5. Re-run read-only server inventory and compare with:
   - `docs/operations/inventory/server-sources.yaml`
   - `docs/operations/inventory/runtime-assets.yaml`
   - `deprecated/worktool/decommission-plan.md`
6. Record any drift before proceeding.

## Read-Only Server Checks

These checks are safe before the cutover because they do not edit server state:

```bash
hostname
date -Iseconds
systemctl status qintopia-message-sidecar.service --no-pager || true
systemctl status qintopia-message-embedding-worker.service --no-pager || true
systemctl status qintopia-message-identity-worker.service --no-pager || true
systemctl --user status worktool-gateway.service --no-pager || true
systemctl --user status hermes-gateway-xiaoqin-worktool.service --no-pager || true
sudo systemctl status qiwe-openclaw-adapter.service --no-pager || true
sudo systemctl status openclaw-embedding-proxy.service --no-pager || true
sudo ss -ltnp | grep -E ':(18557|8787)\b' || true
sudo nginx -T | grep -nE '18557|8787' || true
```

Do not print environment files, tokens, raw profile memory, or private chat logs.

## Monorepo Checkout Preparation

The server target checkout is:

```text
/home/ubuntu/qintopia-agent-os-monorepo
```

Preparation sequence:

```bash
cd /home/ubuntu
test ! -e qintopia-agent-os-monorepo || true
git clone git@github.com:qintopia-agent-studio/qintopia-agent-os.git qintopia-agent-os-monorepo
cd /home/ubuntu/qintopia-agent-os-monorepo
git checkout master
git fetch origin
git checkout <approved-target-sha>
git status --short --branch
git rev-parse HEAD
pnpm install --frozen-lockfile
pnpm check
pnpm deploy:preflight
```

If the checkout already exists, use `git fetch` and
`git checkout <approved-target-sha>`. Do not edit files in place.

## Sidecar Build And Preflight

Build the sidecar from the monorepo checkout:

```bash
cargo build --release --locked --manifest-path runtime/sidecar/Cargo.toml
```

Run local binary checks without changing systemd:

```bash
set -a
. /etc/qintopia/message-sidecar.env
set +a
runtime/sidecar/target/release/qintopia-message-sidecar check
runtime/sidecar/target/release/qintopia-message-sidecar run-embedding-worker --check-only
runtime/sidecar/target/release/qintopia-message-sidecar run-identity-worker --check-only --batch-size 5
deploy/sidecar/scripts/operations-control-plane-smoke.sh
deploy/sidecar/scripts/xiaoman-activity-acceptance-smoke.sh
```

Guarded apply smokes are database-write checks. Run them only when the owner explicitly
approves test audit rows during the window.

## Systemd Cutover

The sidecar service family should point to the monorepo checkout only after preflight
passes.

Expected service shape:

```text
WorkingDirectory=/home/ubuntu/qintopia-agent-os-monorepo
ExecStart=/home/ubuntu/qintopia-agent-os-monorepo/runtime/sidecar/target/release/qintopia-message-sidecar <subcommand>
EnvironmentFile=/etc/qintopia/message-sidecar.env
```

For exact sidecar command details, use `deploy/sidecar/docs/monorepo-cutover-plan.md`
and the adopted `deploy/sidecar/scripts/server-deploy.sh` as reference material. The
script is a legacy snapshot until it is made monorepo-native, so review its environment
defaults before using it.

Minimum service checks after restart:

```bash
sudo systemctl daemon-reload
sudo systemctl restart qintopia-message-sidecar.service
systemctl status qintopia-message-sidecar.service --no-pager
journalctl -u qintopia-message-sidecar.service -n 100 --no-pager
```

Worker and timer restarts should be scoped to the services approved for the migration
window. Do not enable new workers by default.

## Deprecated Runtime Cleanup

Cleanup is allowed only after owner approval during M9.

Deprecated assets currently classified for final-migration handling:

- `/home/ubuntu/worktool-gateway`
- `/home/ubuntu/.hermes/profiles/xiaoqin`
- `/home/ubuntu/.hermes/profiles/xiaoqin/plugins/worktool-platform`
- `/opt/qiwe-openclaw-adapter`
- `worktool-gateway.service`
- `hermes-gateway-xiaoqin-worktool.service`
- `qiwe-openclaw-adapter.service`
- `openclaw-embedding-proxy.service`
- root user `openclaw-gateway.service`
- current nginx references to legacy port `18557`

Archive-first sequence:

1. Record service state and paths before changes.
2. Stop and disable approved legacy units.
3. Move approved legacy directories and unit files to an owner-approved dated archive.
4. Reconcile nginx routes that still point to legacy ports.
5. Re-run service, port, cron, process, and nginx checks.
6. Record archive path and validation evidence in a follow-up git commit.

Do not remove archives permanently until the owner confirms no rollback or audit need
remains.

## Post-Cutover Acceptance

Minimum acceptance checks:

```bash
git rev-parse HEAD
pnpm deploy:preflight
runtime/sidecar/target/release/qintopia-message-sidecar check
deploy/sidecar/scripts/operations-control-plane-smoke.sh
deploy/sidecar/scripts/xiaoman-activity-acceptance-smoke.sh
systemctl is-active qintopia-message-sidecar.service
journalctl -u qintopia-message-sidecar.service -n 100 --no-pager
```

Acceptance evidence must state whether the cutover touched:

- external sends
- database writes
- Hermes profile runtime
- secrets
- Feishu
- QiWe
- systemd
- nginx

## Rollback

Rollback returns the active service family to the previous standalone checkout and
previous binary. It does not undo database migrations unless a separate migration
rollback was approved before the cutover.

Rollback sequence:

```bash
sudo systemctl stop qintopia-message-sidecar.service
sudo systemctl edit --full qintopia-message-sidecar.service
# restore WorkingDirectory=/home/ubuntu/qintopia-msg-sidecar
# restore ExecStart=/home/ubuntu/qintopia-msg-sidecar/target/release/qintopia-message-sidecar run
sudo systemctl daemon-reload
sudo systemctl start qintopia-message-sidecar.service
systemctl status qintopia-message-sidecar.service --no-pager
journalctl -u qintopia-message-sidecar.service -n 100 --no-pager
```

If deprecated runtime assets were archived before rollback, restore only the assets
needed for the rollback path and record why they were restored.

## Follow-Up Commit

After M9, update git with:

- target SHA and operator record
- validation commands and results
- service state after cutover
- cleanup archive paths if any
- rollback status
- changes to `docs/plans/active/monorepo-migration.md`
- `CHANGELOG.md`

Do not leave migration state only in chat or server shell history.
