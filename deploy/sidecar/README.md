# Deploy: Sidecar

`deploy/sidecar` is the rollout, smoke, and rollback contract for the Agent OS sidecar
service family.

## Current Status

`scripts/server-deploy.sh` is a legacy source snapshot adopted from
`qintopia-message-sidecar@eda2652`. It preserves the current standalone sidecar deploy
knowledge, systemd units, smokes, and rollback hints. It is not the monorepo-native
production deployment entrypoint.

M9/M10 moved the approved sidecar service family, active `qintopia-agentos-*` workers,
and Hermes `mcp-context` command references to immutable release directories under
`/home/ubuntu/qintopia-agent-os-releases/<sha>` with stable `current` and `previous`
symlinks.

## Xiaoman Feishu Poster Return

The release bundle installs disabled intake, notification starter, callback, direct
delivery, and internal-group delivery units. The two delivery services share one durable
notification queue and worker binary, but each systemd command pins exactly one
conversation scope. Production direct activation is explicit:

```bash
QINTOPIA_XIAOMAN_FEISHU_POSTER_PRODUCTION_ACTIVATION=approved-production-xiaoman-feishu-poster-return \
  deploy/sidecar/scripts/activate-xiaoman-feishu-poster-production.sh
```

Activation requires persistent `QINTOPIA_XIAOMAN_FEISHU_POSTER_ENABLED=1` in the fixed
sidecar env and `QINTOPIA_XIAOMAN_POSTER_REVIEW_HOOK_ENABLE=1` in the fixed Xiaoman
Hermes env. Both env files must contain the same callback key and must keep the
internal-group switch at `0`; group enablement is accepted only by the separate script
below. The live plugin must resolve to the immutable `release/current` Xiaoman variant.
The script first disables the internal-group delivery timer, runs the release-local
preflight, restarts and verifies `hermes-gateway-xiaoman.service` through the fixed
`ubuntu` user boundary, then starts intake/callback/starter and enables direct delivery
last. A failed gateway restart occurs before any workflow unit or timer is enabled. The
script does not source either env file or print the callback key. Full poster rollback
stops both scoped delivery timers first, requires the direct and group persistent
switches to be `0`, restarts Xiaoman to unload the hook, and retains all workflow,
attempt, notification, and review audit data:

```bash
QINTOPIA_XIAOMAN_FEISHU_POSTER_PRODUCTION_ROLLBACK=approved-production-xiaoman-feishu-poster-return-rollback \
  deploy/sidecar/scripts/rollback-xiaoman-feishu-poster-production.sh
```

### Internal-Group Activation

The shared intake, starter, and callback services continue to run the direct path while
the dedicated internal-group delivery timer remains disabled. After the release is
installed, the private and internal-group policies are applied, and the exact server
chat/user ceilings are reviewed, inspect the disabled boundary without calling Feishu or
Postgres:

```bash
QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_PRODUCTION_OBSERVATION_ENABLE=1 \
QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_EXPECTED_STATE=disabled \
  deploy/sidecar/scripts/xiaoman-feishu-internal-group-production-observation-smoke.sh
```

The observation requires the immutable `release/current` production sidecar and Xiaoman
plugin, matching ingress and callback keys across the sidecar and Hermes environments,
distinct ingress/callback keys, authenticated V3 ingress, and delivery allowlists that
exactly match the ingress deployment ceiling. All delivery users must also be within the
operations reviewer ceiling. It also proves that the direct timer is active and the
group timer has the expected state. It emits only release identity, state, and allowlist
counts.

Set `QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED=1` exactly once in both fixed env
files, then use the separate owner approval to activate one reviewed internal group:

```bash
QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_PRODUCTION_ACTIVATION=approved-production-xiaoman-feishu-internal-group \
  deploy/sidecar/scripts/activate-xiaoman-feishu-internal-group-production.sh
```

Activation first requires the group timer to be stopped while the direct timer remains
active. It runs the group-scoped no-network poster preflight, reloads Xiaoman, intake,
and callback configuration, then enables only the scope-pinned group timer and requires
an enabled-state observation. It does not apply a conversation policy or select a chat.
Pending eligible group notifications may be delivered after that timer starts, so the
command is an external-delivery activation boundary and requires separate owner
approval.

For rollback, first set the persistent group switch to `0` exactly once in both fixed
env files. The guarded rollback stops only the group delivery timer, then proves the
disabled configuration before any gateway or worker reload. It reloads the shared
services while continuously requiring direct delivery to remain active; a failed group
activation cannot disable the direct path:

```bash
QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_PRODUCTION_ROLLBACK=approved-production-xiaoman-feishu-internal-group-rollback \
  deploy/sidecar/scripts/rollback-xiaoman-feishu-internal-group-production.sh
```

Rollback retains policies, participants, workflows, notifications, attempts, and
ambiguous outcomes. It never reroutes a group result to a direct chat or group main
timeline.

## Current Source

- Local source: `../qintopia-message-sidecar/scripts/server-deploy.sh`
- Legacy runbook snapshot:
  `../qintopia-message-sidecar/docs/operations/server-deployment.md`
- Adopted reference: `eda2652f21999e4f32699463413372accbd3b76e`
- Server deployment path observed on 2026-07-03: `/home/ubuntu/qintopia-msg-sidecar`

## Deployment Rule

Server deployment must use git and an approved commit SHA. Do not edit files directly on
the server and do not use `scp` overwrites as a normal release path.

The global M9 execution contract is
`../../docs/operations/m9-server-cutover-runbook.md`. The target server filesystem model
is `../../docs/operations/server-directory-plan.md`. The sidecar-specific historical
cutover notes are in `docs/monorepo-cutover-plan.md`.

The monorepo-native systemd target shape is documented in
`docs/systemd-cutover-plan.md`. Render and validate the unit review files without
touching the server:

```bash
pnpm deploy:systemd:check
```

Legacy-reference removal is documented in `docs/m9f-legacy-reference-removal.md`.
Validate the stable release/current model checks without touching the server:

```bash
pnpm deploy:release-model:check
```

To produce review files under `dist/` for a candidate SHA:

```bash
QINTOPIA_M9_TARGET_SHA="<approved-target-sha>" \
deploy/sidecar/scripts/render-systemd-units.sh
```

## Server Caveat

Production is release/current based. Huabaosi image generation is owner-approved only
through the fixed production feature, release-bound preflight, and explicit timer
activation documented in `docs/server-deployment.md`. Huabaosi WeCom shadow/canary
material remains a separate review boundary.

QiWe image-send production observation is also release/current based. The read-only
production observation smoke accepts only the immutable production sidecar artifact
without QiWe live adapter features, parses only the non-secret send enable flag from the
fixed production env file, and confirms the production apply service/timer is absent,
inactive, and disabled:

```bash
QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_OBSERVATION_ENABLE=1 \
scripts/qiwe-image-send-production-observation-smoke.sh
```

It does not accept production env/release/systemctl overrides, source shell, pass
database/QiWe secrets to a child process, run sidecar commands, run `--apply`, or
process callbacks.

## QiWe Image-Send Staging

Before the send exercise, run the read-only staging readiness smoke on the staging
server. It checks only the fixed staging env path, immutable staging release root,
release SHA, and sidecar digest; it does not read env contents or execute the sidecar:

```bash
QINTOPIA_QIWE_IMAGE_STAGING_READINESS_ENABLE=1 \
QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL=approved-staging-qiwe-image-send \
QINTOPIA_QIWE_IMAGE_STAGING_RELEASE_SHA='<approved staging release sha>' \
QINTOPIA_QIWE_IMAGE_STAGING_SIDECAR_SHA256='<approved staging sidecar binary sha256>' \
scripts/qiwe-image-send-staging-readiness-smoke.sh
```

The two-phase staging smoke is the only reviewed shell entrypoint for a real
`qiwe-staging-adapter` upload and callback send exercise:

```bash
QINTOPIA_QIWE_IMAGE_STAGING_SMOKE_ENABLE=1 \
QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL=approved-staging-qiwe-image-send \
QINTOPIA_QIWE_IMAGE_STAGING_PHASE=preflight \
QINTOPIA_QIWE_IMAGE_STAGING_ENV_FILE=/etc/qintopia/message-sidecar-staging.env \
QINTOPIA_QIWE_IMAGE_STAGING_DATABASE_URL_SHA256='<approved staging database URL sha256>' \
QINTOPIA_QIWE_IMAGE_STAGING_SIDECAR_SHA256='<approved staging sidecar binary sha256>' \
scripts/qiwe-image-send-staging-smoke.sh
```

Then run upload only for the reviewed send-ready work item:

```bash
QINTOPIA_QIWE_IMAGE_STAGING_SMOKE_ENABLE=1 \
QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL=approved-staging-qiwe-image-send \
QINTOPIA_QIWE_IMAGE_STAGING_PHASE=upload \
QINTOPIA_QIWE_IMAGE_STAGING_ENV_FILE=/etc/qintopia/message-sidecar-staging.env \
QINTOPIA_QIWE_IMAGE_STAGING_DATABASE_URL_SHA256='<approved staging database URL sha256>' \
QINTOPIA_QIWE_IMAGE_STAGING_SIDECAR_SHA256='<approved staging sidecar binary sha256>' \
QINTOPIA_QIWE_IMAGE_STAGING_WORK_ITEM_ID='<approved send-ready UUID>' \
scripts/qiwe-image-send-staging-smoke.sh
```

Run the `callback` phase only by streaming one owner-approved callback directly to
stdin:

```bash
trusted-staging-callback-source | \
QINTOPIA_QIWE_IMAGE_STAGING_SMOKE_ENABLE=1 \
QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL=approved-staging-qiwe-image-send \
QINTOPIA_QIWE_IMAGE_STAGING_PHASE=callback \
QINTOPIA_QIWE_IMAGE_STAGING_ENV_FILE=/etc/qintopia/message-sidecar-staging.env \
QINTOPIA_QIWE_IMAGE_STAGING_DATABASE_URL_SHA256='<same approved staging database URL sha256>' \
QINTOPIA_QIWE_IMAGE_STAGING_SIDECAR_SHA256='<same approved staging sidecar binary sha256>' \
QINTOPIA_QIWE_IMAGE_STAGING_WORK_ITEM_ID='<same approved send-ready UUID>' \
scripts/qiwe-image-send-staging-smoke.sh
```

Never persist the callback body or credentials in a file, environment variable,
argument, shell history, report, or log. The wrapper runs only the reviewed packaged
`sidecar/qintopia-message-sidecar` whose SHA-256 matches the command, parses only its
fixed staging env key allowlist without evaluating shell syntax, and preflight/upload
subprocesses receive `/dev/null` instead of the callback stream. Subprocess output is
scanned in memory before the fixed report schema is validated through an anonymous pipe;
the wrapper never writes subprocess output to a file. Successful phases print only fixed
`qiwe_image_send_staging_evidence=<json>` lines and the final pass message. The full
operator checklist is `docs/operations/qiwe-image-send-staging-runbook.md`. This smoke
does not install a listener, service, timer, or production feature build.

## Validation

Before any cutover from this monorepo, the deploy package needs:

- exact target branch and commit SHA
- successful CI workflow run for the target SHA, with the `sidecar-artifact` artifact
  uploaded
- server-side manifest and `SHA256SUMS` verification of the downloaded artifact
- rendered systemd unit review output
- package tests and smokes
- service health checks
- rollback command and owner record
