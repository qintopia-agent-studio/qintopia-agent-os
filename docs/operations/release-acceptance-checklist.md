# Release Acceptance Checklist

Use this checklist before merging a Release Please PR, before publishing its draft
GitHub Release, and immediately after deployment. It is meant to catch the common
failure mode where code is merged and released, but the production runtime still cannot
use the intended capability.

This checklist does not publish a Release, deploy to production, create server-local
configuration, enable timers, write Postgres or Feishu, call providers, call QiWe, or
send externally.

## Before Merging Release Please

- Confirm the Release Please PR head is based on current `origin/master`.
- Confirm the Release Please PR changes only `.release-please-manifest.json` and
  `CHANGELOG.md`.
- Run manual Release Please validation on the exact current PR head:

  ```bash
  gh workflow run ci.yml \
    --ref <release-please-head-branch> \
    -f release_please_pr_number=<pr-number>
  ```

- Confirm the PR-attached `Release Please validation` status is `SUCCESS` on the same
  head SHA that will be merged.
- If Release Please force-updates the branch after new commits land on `master`, discard
  older validation runs and rerun the manual dispatch on the new head.
- Review Release notes and PR body language. Infrastructure or activation-ready releases
  must not claim Xiaoman production completion.

## Before Publishing Draft Release

- Confirm the draft Release tag points to current `origin/master`.
- Confirm the tag includes the deploy bundle and any scripts expected to exist under
  `/home/ubuntu/qintopia-agent-os-releases/current`.
- Confirm `tools/deploy/build-deploy-bundle.mjs` packages those scripts.
- Confirm `tools/deploy/check-deploy-contracts.mjs` guards those scripts.
- If the target Release is meant to unblock staging runtime provisioning, confirm the
  deploy bundle includes:
  - `deploy/sidecar/scripts/render-staging-runtime-env.py`
  - `deploy/sidecar/scripts/staging-runtime-prerequisite-observation-smoke.sh`
  - `deploy/sidecar/scripts/staging-runtime-readiness-evidence-smoke.sh`
  - `docs/operations/message-sidecar-staging-values.template.json`
  - `docs/operations/staging-runtime-provisioning-runbook.md`

## After Deployment

- Confirm `/home/ubuntu/qintopia-agent-os-releases/current` resolves to the target
  Release SHA.
- Confirm systemd units were rendered from the immutable release, not from a mutable
  checkout.
- Confirm the expected release-local scripts exist and are executable under
  `/home/ubuntu/qintopia-agent-os-releases/current`.
- For staging runtime provisioning, confirm the renderer exists at:

  ```text
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/render-staging-runtime-env.py
  ```

- Do not create placeholder env files to satisfy path-only readiness checks.
- Do not paste server-local values, database URLs, tokens, table ids, group ids, raw
  activity records, or callback payloads into reports, PRs, logs, or chat.

## Staging Runtime Acceptance

Before running Huabaosi or QiWe real staging smokes:

1. Owner creates `/etc/qintopia/message-sidecar-staging-values.json` from the reviewed
   non-secret template shape.
2. Renderer validation reports `action_status=staging_env_render_ready`.
3. Owner approval phrase creates `/etc/qintopia/message-sidecar-staging.env`.
4. Unified readiness evidence reports `ready_for_huabaosi_qiwe_staging_smokes`.

Hold if readiness reports any missing env file, missing release root, sidecar hash
mismatch, staging database hash mismatch, unsupported env key, insecure path component,
or sensitive output shape.

## Xiaoman Completion Boundary

For Xiaoman-adjacent releases, classify the Release as `infrastructure`,
`activation-ready`, or `production-complete` before publishing. A Release remains
infrastructure-only while Huabaosi staging final JPEG evidence, QiWe staging
upload/callback/send evidence, cross-flow hash evidence, QiWe production enablement,
Huabaosi production activation, Feishu mirror activation, or one real Xiaoman
activity-to-QiWe group-send arrival is missing.
