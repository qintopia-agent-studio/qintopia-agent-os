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
| Previous sidecar SHA   | `b16c247a19ec751c08de75ae2d312f35b765f317`                   |
| Server start time      | TBD                                                          |
| Rollback decision time | TBD                                                          |
| Rollback owner         | PatrickLiveCool                                              |

The target SHA must pass:

```bash
pnpm check
pnpm deploy:preflight
```

The latest verified candidate before M9.3 was
`416fa9b0ffc8219eaf47c5189c9f56547912342c`. After M9.3 merges, use the next green
`master` SHA from CI as the final target for the approved window.

M9.1 also requires a successful CI workflow run for the target SHA, including both
`check` and `sidecar-artifact`. The server must deploy the CI-built artifact after
verifying `artifact-manifest.json` and `SHA256SUMS`; it should not rebuild the sidecar
with local Node.js, pnpm, or Rust tooling during the migration window.

GitHub retains only the latest two sidecar CI artifacts. Download and verify the
approved target artifact before it is older than the current build plus one rollback
build, or preserve the verified server copy under
`/home/ubuntu/qintopia-agent-os-artifacts/<approved-target-sha>`.

## Preflight Dry Run

2026-07-03 read-only preflight results:

| Check                 | Result                                                                             |
| --------------------- | ---------------------------------------------------------------------------------- |
| Local remote          | `origin` points to `git@github.com:qintopia-agent-studio/qintopia-agent-os.git`    |
| Local pushed branch   | `master` pushed successfully                                                       |
| Local HEAD at check   | `a0658e28aee570c56ce1a932ff68b84b43fbbacb`                                         |
| GitHub default branch | `master`                                                                           |
| Server target path    | `/home/ubuntu/qintopia-agent-os-monorepo` missing                                  |
| Server GitHub access  | pass through bot account and `github-qintopia-agent-os` SSH alias                  |
| Server Node.js        | missing from `PATH`; not required for artifact-based M9.1                          |
| Server pnpm           | missing from `PATH`; not required for artifact-based M9.1                          |
| Server Rust           | `cargo 1.96.1`, `rustc 1.96.1` observed; not required for artifact deploy          |
| Server artifact tools | `curl`, `jq`, `unzip`, `sha256sum`, `python3`, `openssl`, and `tar` available      |
| Server disk           | `/` at about 50% used, about 29G available after cleanup                           |
| Current sidecar       | active, enabled, running from `/home/ubuntu/qintopia-msg-sidecar`                  |
| Current sidecar SHA   | `b16c247a19ec751c08de75ae2d312f35b765f317` on `codex/huabaosi-localization-shadow` |

Blocking items before cutover:

1. Confirm the approved target SHA has a successful CI workflow run with the
   `sidecar-artifact` artifact uploaded.
2. Provide GitHub App credentials for private repository artifact download:
   `GITHUB_APP_ID`, `GITHUB_APP_INSTALLATION_ID`, and a server-local private key path.
3. Reconfirm whether the Huabaosi shadow branch should remain review-pool before the
   active service is repointed.

2026-07-04 M9-A and M9-B results:

| Check                  | Result                                                                                       |
| ---------------------- | -------------------------------------------------------------------------------------------- |
| Target SHA             | `1a5351d2d20ae58f0718b24876e4487f8af1d935`                                                   |
| CI workflow            | `28693411837` passed for the target SHA                                                      |
| Server target checkout | `/home/ubuntu/qintopia-agent-os-monorepo` exists and is detached at the target SHA           |
| Server artifact copy   | `/home/ubuntu/qintopia-agent-os-artifacts/1a5351d2d20ae58f0718b24876e4487f8af1d935` verified |
| Artifact checksum      | `sha256sum -c SHA256SUMS` passed                                                             |
| Artifact binary        | `qintopia-message-sidecar`, `linux-x86_64-gnu`, about 25M                                    |
| Sidecar check          | passed                                                                                       |
| Embedding check-only   | passed                                                                                       |
| Identity check-only    | passed when run with the same profile env file that the current systemd unit loads           |
| Operations fixture     | passed without production database env                                                       |
| Xiaoman fixture        | passed                                                                                       |
| Current service wiring | unchanged; active services still point to `/home/ubuntu/qintopia-msg-sidecar/target/release` |

M9-B found one database blocker:

- production Postgres is missing the operations control-plane migration
  `202606300007_operations_control_plane.sql`
- production Postgres is missing the follow-up human actor guard migration
  `202607020001_operations_human_actor_guards.sql`

2026-07-04 M9-C resolved the database blocker:

| Check                          | Result                                                                                        |
| ------------------------------ | --------------------------------------------------------------------------------------------- |
| Migration source               | `/home/ubuntu/qintopia-agent-os-monorepo/runtime/postgres/migrations`                         |
| Migration binary               | verified sidecar artifact at `1a5351d2d20ae58f0718b24876e4487f8af1d935`                       |
| Migration result               | missing AgentOS operations migrations applied                                                 |
| Postgres schema preflight      | passed                                                                                        |
| Operations capability seed     | passed with `capability_count=4`                                                              |
| Worker check-only and dry-runs | passed for embedding, identity, member profile, graph, event signal, archive, digest, and ops |
| Current service wiring         | unchanged; active services still point to `/home/ubuntu/qintopia-msg-sidecar/target/release`  |

Do not repoint systemd until the final target SHA is approved, the server checkout is
updated to that SHA, and `deploy/sidecar/scripts/postgres-schema-preflight.sh` passes
again immediately before the service window.

Production external adapters are still not ready for real sends or real workbench
integration. `operations-readiness-check --profile production` currently reports missing
allowlist/config entries for group targets, reviewers, confirmers, owners, and
attachment hosts. Keep external send and workbench adapter paths disabled until those
values are configured and reviewed.

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
git clone git@github-qintopia-agent-os:qintopia-agent-studio/qintopia-agent-os.git qintopia-agent-os-monorepo
cd /home/ubuntu/qintopia-agent-os-monorepo
git checkout master
git fetch origin
git checkout <approved-target-sha>
git status --short --branch
git rev-parse HEAD
```

If the checkout already exists, use `git fetch` and
`git checkout <approved-target-sha>`. Do not edit files in place.

## Sidecar Artifact Fetch And Preflight

Fetch the CI-built artifact for the approved commit SHA:

```bash
export GITHUB_APP_ID="<github-app-id>"
export GITHUB_APP_INSTALLATION_ID="<installation-id>"
export GITHUB_APP_PRIVATE_KEY_PATH="/etc/qintopia/github-app/qintopia-agent-os-deployer.pem"
deploy/sidecar/scripts/fetch-ci-artifact.sh \
  --sha <approved-target-sha> \
  --output-dir /home/ubuntu/qintopia-agent-os-artifacts/<approved-target-sha>
```

The GitHub App must be installed only on `qintopia-agent-studio/qintopia-agent-os` with
`Actions: read` and `Metadata: read`. `GITHUB_TOKEN` remains a fallback for emergency
artifact downloads, but the normal M9 path should use the GitHub App so releases do not
depend on hand-created personal tokens.

Run binary checks without changing systemd:

```bash
set -a
. /etc/qintopia/message-sidecar.env
. /home/ubuntu/.hermes/profiles/erhua/.env
set +a
ARTIFACT_DIR=/home/ubuntu/qintopia-agent-os-artifacts/<approved-target-sha>
export QINTOPIA_SIDECAR_BIN="${ARTIFACT_DIR}/qintopia-message-sidecar"
"${ARTIFACT_DIR}/qintopia-message-sidecar" check
"${ARTIFACT_DIR}/qintopia-message-sidecar" run-embedding-worker --check-only
"${ARTIFACT_DIR}/qintopia-message-sidecar" run-identity-worker --check-only --batch-size 5
deploy/sidecar/scripts/operations-control-plane-smoke.sh
deploy/sidecar/scripts/xiaoman-activity-acceptance-smoke.sh
```

Run the read-only Postgres schema gate before any service repoint:

```bash
deploy/sidecar/scripts/postgres-schema-preflight.sh
```

If it reports missing migrations, stop and run the database migration step only after
owner approval. The schema gate does not apply migrations and does not read business
rows.

`operations-control-plane-smoke.sh` is a fixture smoke and should run without production
database env. Use it to validate command behavior, then validate the production database
separately with `postgres-schema-preflight.sh`, DB-backed capability checks, and worker
check-only or dry-run commands.

Guarded apply smokes are database-write checks. Run them only when the owner explicitly
approves test audit rows during the window.

## Systemd Cutover

The sidecar service family should point to the monorepo checkout only after preflight
passes.

Expected service shape:

```text
WorkingDirectory=/home/ubuntu/qintopia-agent-os-monorepo
ExecStart=/home/ubuntu/qintopia-agent-os-artifacts/<approved-target-sha>/qintopia-message-sidecar <subcommand>
EnvironmentFile=/etc/qintopia/message-sidecar.env
```

For exact sidecar command details, use `deploy/sidecar/docs/systemd-cutover-plan.md` and
render the target unit review files before copying anything into `/etc/systemd/system`:

```bash
QINTOPIA_M9_TARGET_SHA="<approved-target-sha>" \
deploy/sidecar/scripts/render-systemd-units.sh
```

`deploy/sidecar/scripts/server-deploy.sh` remains a legacy snapshot. Do not use it as
the M9 artifact-based installer without converting and reviewing it first.

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
ARTIFACT_DIR=/home/ubuntu/qintopia-agent-os-artifacts/<approved-target-sha>
cd "$ARTIFACT_DIR" && sha256sum -c SHA256SUMS
export QINTOPIA_SIDECAR_BIN="${ARTIFACT_DIR}/qintopia-message-sidecar"
"${ARTIFACT_DIR}/qintopia-message-sidecar" check
deploy/sidecar/scripts/postgres-schema-preflight.sh
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
