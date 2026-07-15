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

## QiWe Image-Send Staging

The two-phase staging smoke is the only reviewed shell entrypoint for a real
`qiwe-staging-adapter` upload and callback send exercise:

```bash
QINTOPIA_QIWE_IMAGE_STAGING_SMOKE_ENABLE=1 \
QINTOPIA_QIWE_IMAGE_STAGING_PHASE=upload \
QINTOPIA_QIWE_IMAGE_STAGING_ENV_FILE=/etc/qintopia/message-sidecar-staging.env \
QINTOPIA_QIWE_IMAGE_STAGING_DATABASE_URL_SHA256='<approved staging database URL sha256>' \
QINTOPIA_QIWE_IMAGE_STAGING_WORK_ITEM_ID='<approved send-ready UUID>' \
scripts/qiwe-image-send-staging-smoke.sh
```

Run the `callback` phase only by streaming one owner-approved callback directly to
stdin. Never persist the callback body or credentials in a file, environment variable,
argument, shell history, report, or log. The wrapper parses only its fixed staging env
key allowlist without evaluating shell syntax, and preflight/upload subprocesses receive
`/dev/null` instead of the callback stream. Subprocess output is scanned in memory
before the fixed report schema is validated through an anonymous pipe; the wrapper never
writes subprocess output to a file. This smoke does not install a listener, service,
timer, or production feature build.

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
