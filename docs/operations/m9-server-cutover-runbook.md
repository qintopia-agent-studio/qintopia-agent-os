# M9 Server Cutover Runbook

M9 moves production deployment from scattered server checkouts and ad hoc updates to
this monorepo. The approved M9-D active service cutover was executed on 2026-07-04 for
the three sidecar services listed below. The server is currently in a transitional
state: some services run from verified CI artifacts, while legacy workers and Hermes MCP
commands still reference the old `/home/ubuntu/qintopia-msg-sidecar` checkout.

This document is the evidence record for the cutover and the reusable runbook for future
approved repoints or cleanup windows. The target direction is described in
`docs/operations/server-directory-plan.md`: immutable release directories with
`current`/`previous` symlinks should replace direct artifact paths after M9-F.

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

M9-D executed with these inputs:

| Field                  | Value                                                        |
| ---------------------- | ------------------------------------------------------------ |
| Owner approver         | PatrickLiveCool                                              |
| Migration operator     | Codex with owner-provided approval window                    |
| Reviewers              | detroxryo, noraincode, PatrickLiveCool, qiaopengjun5162      |
| Monorepo remote        | `git@github.com:qintopia-agent-studio/qintopia-agent-os.git` |
| Target branch          | master                                                       |
| Target commit SHA      | `c70378408c53de5f4166e8b9bde45b15a97cabb0`                   |
| Previous sidecar SHA   | `b16c247a19ec751c08de75ae2d312f35b765f317`                   |
| Server start time      | 2026-07-04                                                   |
| Rollback decision time | not used; post-cutover checks passed                         |
| Rollback owner         | PatrickLiveCool                                              |

Future repoints must use a new approved target SHA that passes:

```bash
pnpm check
pnpm deploy:preflight
```

The M9-D target SHA was `c70378408c53de5f4166e8b9bde45b15a97cabb0`, with workflow run
`28700602736` passing both `check` and `sidecar-artifact`.

M9.1 also requires a successful CI workflow run for the target SHA, including both
`check` and `sidecar-artifact`. The server must deploy the CI-built artifact after
downloading it from Tencent COS and verifying `artifact-manifest.json` plus
`SHA256SUMS`; it should not rebuild the sidecar with local Node.js, pnpm, or Rust
tooling during the migration window.

GitHub retains only the latest two sidecar CI artifacts for audit and emergency
fallback. COS is the default production distribution path. Preserve the verified server
copy under `/home/ubuntu/qintopia-agent-os-artifacts/<approved-target-sha>` once it is
downloaded.

`/home/ubuntu/qintopia-agent-os-artifacts/<sha>` is a transition path. The next release
model should promote verified payloads into
`/home/ubuntu/qintopia-agent-os-releases/<sha>` and point systemd/Hermes managed mounts
at `/home/ubuntu/qintopia-agent-os-releases/current`.

## Preflight And Cutover Evidence

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

M9-A through M9-C blocking items before cutover:

1. Confirmed the approved target SHA had a successful CI workflow run with the
   `sidecar-artifact` artifact uploaded.
2. Installed GitHub App credentials for private repository artifact download:
   `GITHUB_APP_ID=4214034`, `GITHUB_APP_INSTALLATION_ID=144332887`, and a server-local
   private key path at `/etc/qintopia/github-app/qintopia-agent-os-deployer.pem`.
3. Kept the Huabaosi shadow branch in review-pool; M9-D did not approve it as product
   direction.

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

2026-07-04 M9-D cut over the approved active service family:

| Check                   | Result                                                                                                      |
| ----------------------- | ----------------------------------------------------------------------------------------------------------- |
| Target SHA              | `c70378408c53de5f4166e8b9bde45b15a97cabb0`                                                                  |
| CI workflow             | `28700602736` passed for `check` and `sidecar-artifact`                                                     |
| GitHub App download     | passed with installation `144332887` and server-local private key                                           |
| Server checkout         | `/home/ubuntu/qintopia-agent-os-monorepo` detached at the target SHA                                        |
| Server artifact         | `/home/ubuntu/qintopia-agent-os-artifacts/c70378408c53de5f4166e8b9bde45b15a97cabb0`                         |
| Updated systemd units   | `qintopia-message-sidecar`, `qintopia-message-embedding-worker`, `qintopia-message-identity-worker`         |
| Backup path             | `/home/ubuntu/qintopia-agent-os-backups/m9-systemd-20260704T084453Z`                                        |
| Post-cutover checks     | checksum, sidecar check, embedding check-only, identity check-only, schema preflight, fixture smokes passed |
| Still disabled/deferred | operations timers, real external send/workbench adapters, WorkTool/Xiaoqin/OpenClaw cleanup                 |

The first sidecar restart exposed a missing `QINTOPIA_SIDECAR_MIGRATIONS_DIR` in the
rendered unit. The unit was patched to
`/home/ubuntu/qintopia-agent-os-monorepo/runtime/postgres/migrations`, restarted, and
verified active. Keep this environment line in all future rendered sidecar service
units.

2026-07-04 M9-E migrated repository fetch verification to GitHub App credentials:

| Check                    | Result                                                                                 |
| ------------------------ | -------------------------------------------------------------------------------------- |
| Required App permissions | `Actions: read`, `Contents: read`, `Metadata: read`                                    |
| Contents API             | `README.md` returned `200` with the installation token                                 |
| Server git auth          | `git ls-remote` passed through temporary `GIT_ASKPASS` and the server-local App key    |
| Verified master SHA      | `60cfeadbd972aa4b6a32c76d794cb42f0bc11568`                                             |
| Persistent credential    | none; token is short-lived and not stored in the remote URL, git config, or shell args |

2026-07-04 read-only follow-up identified the remaining mixed-state references:

| Area                        | Current state                                                                                                                                    | Required follow-up                         |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------ |
| New sidecar services        | `qintopia-message-sidecar`, `qintopia-message-embedding-worker`, and `qintopia-message-identity-worker` run from artifact `c703784...`           | keep monitored until next approved repoint |
| Legacy AgentOS workers      | six `qintopia-agentos-*` workers still run from `/home/ubuntu/qintopia-msg-sidecar/target/release/qintopia-message-sidecar`                      | M9-F repoint to approved artifact/release  |
| Hermes MCP context commands | live `mcp-context` processes still start from `/home/ubuntu/qintopia-msg-sidecar`                                                                | move to monorepo/release-managed command   |
| Server checkout             | `/home/ubuntu/qintopia-agent-os-monorepo` is on the latest `master` checkout, but runtime artifact remains pinned to the approved production SHA | do not auto-promote docs-only commits      |
| Directory cleanup           | old checkouts, WorkTool, Xiaoqin, OpenClaw, migration, and worklog guard directories still exist                                                 | archive only after no references remain    |

M9 is therefore not finished as a full server cleanup. It is safe only to say the first
approved service family cutover passed. Complete M9-F before removing
`/home/ubuntu/qintopia-msg-sidecar`.

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
test -d qintopia-agent-os-monorepo
cd /home/ubuntu/qintopia-agent-os-monorepo
git checkout master
git remote set-url origin https://github.com/qintopia-agent-studio/qintopia-agent-os.git
GITHUB_APP_ID=4214034 \
GITHUB_APP_INSTALLATION_ID=144332887 \
GITHUB_APP_PRIVATE_KEY_PATH=/etc/qintopia/github-app/qintopia-agent-os-deployer.pem \
deploy/sidecar/scripts/github-app-git.sh -- fetch origin
git checkout <approved-target-sha>
git status --short --branch
git rev-parse HEAD
```

If the checkout already exists, use `git fetch` and `git checkout <approved-target-sha>`
through `github-app-git.sh`. Keep `origin` as the plain HTTPS repository URL without
embedded credentials:

```bash
git remote set-url origin https://github.com/qintopia-agent-studio/qintopia-agent-os.git
```

Do not store the installation token in `.git/config`, shell history, or a credential
helper cache.

This sequence assumes the current server checkout already exists. For a brand-new host,
bootstrap the first checkout with a separately reviewed copy of
`deploy/sidecar/scripts/github-app-git.sh` or a dedicated bootstrap runbook; do not
reintroduce a long-lived bot credential just to perform the first clone.

## Sidecar Artifact Fetch And Preflight

Fetch the CI-built artifact for the approved commit SHA from Tencent COS:

```bash
export TENCENT_COS_BUCKET="qintopia-agent-os-artifacts-1305166808"
export TENCENT_COS_REGION="ap-shanghai"
export TENCENT_COS_PREFIX="qintopia-agent-os"
export TENCENT_COS_AUTH_MODE="CvmRole"
export TENCENT_COS_CVM_ROLE_NAME="<cvm-role-name>"
deploy/sidecar/scripts/fetch-cos-artifact.sh \
  --sha <approved-target-sha> \
  --output-dir /home/ubuntu/qintopia-agent-os-artifacts/<approved-target-sha>
```

If CVM Role is unavailable, use a read-only COS SecretId/SecretKey on the server. GitHub
artifact download through `fetch-ci-artifact.sh` remains an emergency fallback only; do
not use `scp`, direct server edits, or long-lived bot credentials for production
artifact distribution.

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

Current transition service shape:

```text
WorkingDirectory=/home/ubuntu/qintopia-agent-os-monorepo
ExecStart=/home/ubuntu/qintopia-agent-os-artifacts/<approved-target-sha>/qintopia-message-sidecar <subcommand>
EnvironmentFile=/etc/qintopia/message-sidecar.env
Environment=QINTOPIA_SIDECAR_MIGRATIONS_DIR=/home/ubuntu/qintopia-agent-os-monorepo/runtime/postgres/migrations
```

Target release/current service shape:

```text
WorkingDirectory=/home/ubuntu/qintopia-agent-os-releases/current
ExecStart=/home/ubuntu/qintopia-agent-os-releases/current/sidecar/qintopia-message-sidecar <subcommand>
EnvironmentFile=/etc/qintopia/message-sidecar.env
Environment=QINTOPIA_SIDECAR_MIGRATIONS_DIR=/home/ubuntu/qintopia-agent-os-releases/current/runtime/postgres/migrations
Environment=QINTOPIA_DEPLOYED_COMMIT_SHA=<approved-target-sha>
```

For M9-F, repoint the remaining already-active legacy workers first. Do not enable the
operations timers or external adapter paths during the same window.

M9-F exact worker scope:

| Unit                                               | Command                             |
| -------------------------------------------------- | ----------------------------------- |
| `qintopia-agentos-member-profile-worker.service`   | `run-member-profile-worker`         |
| `qintopia-agentos-graph-projection-worker.service` | `run-graph-projection-worker`       |
| `qintopia-agentos-raw-archive-worker.service`      | `run-raw-archive-worker`            |
| `qintopia-agentos-event-signal-worker.service`     | `run-event-signal-worker`           |
| `qintopia-agentos-daily-digest-worker.service`     | `agentos-daily-digest-worker`       |
| `qintopia-agentos-daily-digest-publisher.service`  | `run-daily-digest-publisher-worker` |

Use `deploy/sidecar/docs/m9f-legacy-reference-removal.md` for the M9-F read-only
preflight, Hermes `mcp-context` wrapper migration, apply sequence, rollback, and cleanup
deferral.

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

Cleanup is allowed only after owner approval and after M9-F removes live references to
legacy paths.

Deprecated assets currently classified for final-migration handling:

- `/home/ubuntu/qintopia-msg-sidecar`
- `/home/ubuntu/qintopia-agent-os`
- `/home/ubuntu/qintopia-hermes-runtime`
- `/home/ubuntu/qintopia-message-sidecar-build`
- `/home/ubuntu/qintopia-artifacts`
- `/home/ubuntu/qintopia-migration`
- `/home/ubuntu/qintopia-worklog-guard-*`
- `/home/ubuntu/worktool-gateway`
- `/home/ubuntu/worktool-gateway-old`
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
