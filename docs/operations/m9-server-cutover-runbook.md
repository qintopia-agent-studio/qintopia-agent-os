# M9 Server Cutover Runbook

M9 moves production deployment from scattered server checkouts and ad hoc updates to
this monorepo. The approved M9-D active service cutover was executed on 2026-07-04 for
the three sidecar services listed below. M9-F then moved the remaining sidecar worker
runtime and Hermes `qintopia-context` MCP command onto the release/current model. The
server is still transitional for profile/plugin bundles and directory cleanup, but the
active sidecar runtime no longer depends on the old `/home/ubuntu/qintopia-msg-sidecar`
checkout.

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

GitHub retains the latest ten sidecar CI artifacts for audit and emergency fallback. COS
is the default production distribution path. Preserve the verified server copy under
`/home/ubuntu/qintopia-agent-os-artifacts/<approved-target-sha>` once it is downloaded.

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

| Area                        | Current state                                                                                                                          | Required follow-up                            |
| --------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------- |
| New sidecar services        | `qintopia-message-sidecar`, `qintopia-message-embedding-worker`, and `qintopia-message-identity-worker` run from artifact `c703784...` | keep monitored until next approved repoint    |
| Legacy AgentOS workers      | six `qintopia-agentos-*` workers still run from `/home/ubuntu/qintopia-msg-sidecar/target/release/qintopia-message-sidecar`            | M9-F repoint to approved artifact/release     |
| Hermes MCP context commands | live `mcp-context` processes still start from `/home/ubuntu/qintopia-msg-sidecar`                                                      | move to deploy-bundle/release-managed command |
| Server checkout             | `/home/ubuntu/qintopia-agent-os-monorepo` is a transition checkout and may lag behind `master`                                         | do not use git fetch as the release path      |
| Directory cleanup           | old checkouts, WorkTool, Xiaoqin, OpenClaw, migration, and worklog guard directories still exist                                       | archive only after no references remain       |

M9-D was not enough to remove `/home/ubuntu/qintopia-msg-sidecar`; M9-F was required to
move the remaining worker and MCP references first.

2026-07-05 M9-F cut over the six already-active AgentOS worker services:

| Check                 | Result                                                                                                |
| --------------------- | ----------------------------------------------------------------------------------------------------- |
| Target release SHA    | `13a3957369ad80ea8b6e93d4c67c6ef120ecffd6`                                                            |
| Artifacts workflow    | `28740196040` passed for `sidecar-artifact` and `deploy-bundle-artifact`                              |
| Release directory     | `/home/ubuntu/qintopia-agent-os-releases/13a3957369ad80ea8b6e93d4c67c6ef120ecffd6`                    |
| Current symlink       | `/home/ubuntu/qintopia-agent-os-releases/current -> 13a3957369ad80ea8b6e93d4c67c6ef120ecffd6`         |
| Backup path           | `/home/ubuntu/qintopia-agent-os-backups/m9f-systemd-20260705T122149Z`                                 |
| Updated systemd units | the six `qintopia-agentos-*` worker services listed in the M9-F exact worker scope                    |
| Post-cutover checks   | six workers active/enabled, zero restarts after cutover, executable paths resolve through the SHA dir |
| Binary check          | release binary `check` passed NATS JetStream and Postgres checks                                      |
| Still deferred        | Hermes MCP command repoint, legacy directory cleanup, operations timers, real external send paths     |

The six worker unit files no longer reference `/home/ubuntu/qintopia-msg-sidecar`. They
point to `/home/ubuntu/qintopia-agent-os-releases/current` for `WorkingDirectory`,
`ExecStart`, and `QINTOPIA_SIDECAR_MIGRATIONS_DIR`.

2026-07-05 later in M9-F completed the remaining release/current runtime repoint:

| Check                   | Result                                                                                                  |
| ----------------------- | ------------------------------------------------------------------------------------------------------- |
| Hermes MCP backup       | `/home/ubuntu/qintopia-agent-os-backups/m9f-hermes-mcp-20260705T123637Z`                                |
| Message systemd backup  | `/home/ubuntu/qintopia-agent-os-backups/m9f-message-systemd-20260705T123732Z`                           |
| Updated Hermes profiles | Erhua and Wenyuange `qintopia-context` commands now use the release-managed wrapper                     |
| Updated message units   | `qintopia-message-sidecar`, `qintopia-message-embedding-worker`, and `qintopia-message-identity-worker` |
| Active runtime shape    | all nine sidecar/worker services use `/home/ubuntu/qintopia-agent-os-releases/current`                  |
| Runtime SHA             | `13a3957369ad80ea8b6e93d4c67c6ef120ecffd6`                                                              |
| Process reference check | old sidecar checkout processes `0`; transition artifact processes `0`; release/current processes `13`   |
| Binary checks           | production `check`, embedding `--check-only`, and identity `--check-only --batch-size 5` passed         |
| Still deferred          | real `previous` symlink, profile/plugin bundles, legacy directory archive, external send/workbench      |

The old sidecar checkout is now eligible for the later archive-readiness audit, but it
must not be deleted until process, unit, timer, cron, MCP, nginx, rollback, and evidence
paths are checked again in that cleanup window.

## Pre-Cutover Freeze

Before any server mutation:

1. Confirm direct server edits for Agent OS, sidecar, Hermes profile templates, and
   legacy WorkTool/OpenClaw paths remain frozen.
2. Confirm no one is making `scp` source updates, hotfixes, or direct server commits.
3. Confirm local `master` is clean and pushed to the approved remote.
4. Confirm the runtime artifact and deploy bundle have successful CI for their approved
   SHA values.
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

## Release Source Preparation

For routine runtime releases, the server should not pull source code from GitHub. The
release source is the verified artifact from Tencent COS:

```text
cos://qintopia-agent-os-artifacts/qintopia-agent-os/sidecar/<approved-target-sha>/
cos://qintopia-agent-os-artifacts/qintopia-agent-os/deploy-bundle/<approved-deploy-bundle-sha>/
```

The deploy bundle contains reviewed operator files such as the Hermes MCP wrapper,
systemd renderer, and M9-F runbooks. It replaces the previous M9-F assumption that the
server deploy checkout had to be updated before wrapper or unit changes.

The server checkout exists only as a transition diagnostics/bootstrap checkout. It is
not the normal release transport. Do not run `git fetch` or `git checkout` just to
deploy a new runtime artifact, wrapper, renderer, or systemd template.

Use GitHub App based git access only for these explicit cases:

- bootstrapping a new deploy checkout on a new host
- read-only repository reachability diagnostics
- emergency fallback when the owner approves using the repo instead of COS

Keep `origin` as the plain HTTPS repository URL without embedded credentials:

```bash
cd /home/ubuntu/qintopia-agent-os-monorepo
git remote set-url origin https://github.com/qintopia-agent-studio/qintopia-agent-os.git
```

Do not store the installation token in `.git/config`, shell history, or a credential
helper cache.

For a brand-new host, bootstrap the first checkout with a separately reviewed copy of
the deploy runner or a dedicated bootstrap runbook. Do not reintroduce a long-lived bot
credential just to perform the first clone.

## Sidecar Artifact Fetch And Preflight

Fetch the CI-built artifact for the approved commit SHA from Tencent COS. On CVM hosts,
use CVM Role mode:

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

Fetch the M9-F deploy bundle from COS:

```bash
set -a
. /etc/qintopia/cos-artifacts.env
set +a
deploy/sidecar/scripts/fetch-cos-artifact.sh \
  --artifact-type deploy-bundle \
  --sha <approved-deploy-bundle-sha> \
  --output-dir /tmp/qintopia-agent-os-deploy-bundle/<approved-deploy-bundle-sha>
```

For Tencent Cloud Lighthouse app servers, use the server-local read-only COS environment
file instead:

```bash
set -a
. /etc/qintopia/cos-artifacts.env
set +a
deploy/sidecar/scripts/fetch-cos-artifact.sh \
  --sha <approved-target-sha> \
  --output-dir /home/ubuntu/qintopia-agent-os-artifacts/<approved-target-sha>
```

GitHub artifact download through `fetch-ci-artifact.sh` remains an emergency fallback
only; do not use `scp`, direct server edits, or long-lived bot credentials for
production artifact distribution.

For read-only transport validation, download to `/tmp` instead of production artifact
directories and stop after manifest/checksum checks:

```bash
set -a
. /etc/qintopia/cos-artifacts.env
set +a
deploy/sidecar/scripts/fetch-cos-artifact.sh \
  --sha <approved-target-sha> \
  --output-dir /tmp/qintopia-agent-os-cos-readonly/<approved-target-sha>

deploy/sidecar/scripts/fetch-cos-artifact.sh \
  --artifact-type deploy-bundle \
  --sha <approved-deploy-bundle-sha> \
  --output-dir /tmp/qintopia-agent-os-deploy-bundle-readonly/<approved-deploy-bundle-sha>
```

If the transitional server checkout does not yet contain the current COS fetch scripts,
do not run `git fetch` just to obtain them. Copy the approved `fetch-cos-artifact.sh`
and `install-coscli.sh` scripts for the approved commit into a temporary `/tmp`
bootstrap directory, run the read-only fetch from there, and record the script source
SHA in the migration evidence.

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

## Release Assembly And Systemd Cutover

The sidecar service family should point to the release/current directory only after
preflight passes. Do not use `/home/ubuntu/qintopia-agent-os-deploy-bundles/<sha>` as a
production `WorkingDirectory`; the deploy bundle is an input used to assemble the
immutable release directory.

M9-F release/current service shape:

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
RELEASE_DIR="/home/ubuntu/qintopia-agent-os-releases/<approved-release-sha>"
"${RELEASE_DIR}/deploy/sidecar/scripts/render-systemd-units.sh" \
  --target-sha "<approved-runtime-sha>" \
  --artifact-dir "/home/ubuntu/qintopia-agent-os-releases/current/sidecar" \
  --monorepo-dir "/home/ubuntu/qintopia-agent-os-releases/current" \
  --migrations-dir "/home/ubuntu/qintopia-agent-os-releases/current/runtime/postgres/migrations" \
  --output-dir "/tmp/qintopia-m9f-rendered-units"
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

After a similar future cutover, update git with:

- target SHA and operator record
- validation commands and results
- service state after cutover
- cleanup archive paths if any
- rollback status
- changes to the current roadmap, package docs, or operation record
- `CHANGELOG.md`

Do not leave migration state only in chat or server shell history.
